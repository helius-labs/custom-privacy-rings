#!/usr/bin/env bash
# The ring devnet stack on ECS, every AWS name and tag under zolana-ring-devnet.
#
#   tools/ring-devnet-deploy.sh up [--dry-run]               build missing images, then create or update the stack
#   tools/ring-devnet-deploy.sh status [--dry-run]           endpoints, custody keys, demo env and readiness
#   tools/ring-devnet-deploy.sh down [--purge] [--dry-run]   delete the stack, --purge also its KMS keys, secrets and images
#
# --dry-run prints each aws call, runs none and changes nothing. tools/ring-devnet.md
# describes the stack. Needs aws (with write access), docker, jq, openssl, python3, git, curl.
#
# Environment
#   RING_DEVNET_PHOTON_START_SLOT       first slot Photon indexes, required while its database is absent
#   RING_DEVNET_RING_INDEXER_START_SLOT first slot of a new ring index, default latest
#   RING_DEVNET_RPC_URL                 Solana RPC the services read, stored in a secret, default the public devnet RPC
#   RING_DEVNET_PUBLIC_RPC_URL          Solana RPC status prints, default https://api.devnet.solana.com
#   RING_DEVNET_ALLOW_ORIGINS           browser origins of the Ring RPC, default http://localhost:3000
#   RING_DEVNET_WEBAUTHN_RP_ID          WebAuthn RP id over every origin, default the host of the first origin
#   RING_DEVNET_RELAY_BUDGET_LAMPORTS   default 10000000000, zero is refused
#   RING_DEVNET_RELAY_SIGNER_BUDGET_LAMPORTS  default 100000000, zero is refused
#   RING_DEVNET_ENROLLMENT_BUDGET_LAMPORTS    default 1000000000, zero is refused
#   RING_DEVNET_ENROLLMENT_RING_BUDGET_LAMPORTS  default 20000000 per ring, zero is refused
#   RING_DEVNET_ENROLLMENT_MAX_RINGS    rings that may register, default 64, zero is refused
#   RING_DEVNET_CO_SIGNER_APPROVE       default outflow,withdrawal,transfers,deposits,withdrawals
#   RING_DEVNET_READER_GRANTS           tree or database, default database
#   RING_DEVNET_PHOTON_BLOCK_FETCHES    blocks Photon fetches at once, default Photon's own
#   RING_DEVNET_PHOTON_CPU, RING_DEVNET_PHOTON_MEMORY      default 2048, 8192
#   RING_DEVNET_RING_RPC_CPU, RING_DEVNET_RING_RPC_MEMORY  Ring RPC and prover task, default 4096, 16384
#   RING_DEVNET_DB_CLASS                default db.t4g.micro
#   RING_DEVNET_RING_RPC_TAG, RING_DEVNET_RING_INDEXER_TAG, RING_DEVNET_PROVER_TAG, RING_DEVNET_PHOTON_TAG
#                                       image tags, default <service>-<commit12>
#   RING_DEVNET_VPC                     VPC id, default the account's default VPC
#   RING_DEVNET_ORIGIN_CERT_ARN         ACM certificate ARN for TLS listeners and https-only CloudFront origins
#   RING_DEVNET_ORIGIN_DOMAIN           custom origin domain alias matching RING_DEVNET_ORIGIN_CERT_ARN
#   AWS_REGION                          default eu-north-1
set -euo pipefail

usage() {
    awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$0" >&2
    exit 2
}

dry_run=0
purge=0
command=""
for arg in "$@"; do
    case "$arg" in
        --dry-run) dry_run=1 ;;
        --purge) purge=1 ;;
        up | status | down) [[ -z "$command" ]] || usage; command="$arg" ;;
        *) usage ;;
    esac
done
[[ -n "$command" ]] || usage
[[ $purge -eq 0 || "$command" == down ]] || usage

# Dry run lines go to fd 3, a caller redirect of stderr cannot hide them.
exec 3>&2
root="$(git rev-parse --show-toplevel)"
cd "$root"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

region="${AWS_REGION:-eu-north-1}"
prefix="zolana-ring-devnet"
tag_spec="Key=$prefix,Value=1"
ecs_tag_spec="key=$prefix,value=1"
cluster="$prefix"
role="$prefix-exec"
task_role="$prefix-ring-rpc-task"
log_group="/$prefix"
load_balancer="$prefix"
security_group="$prefix-sg"
db_security_group="$prefix-db-sg"
db_subnet_group="$prefix"
db_class="${RING_DEVNET_DB_CLASS:-db.t4g.micro}"
# zolnet-devnet serves its Photon and prover on it, no command here deletes it.
protected_distribution="E7DAYCSYTCIWK"
photon_port=8784
ring_rpc_port=8785
ring_indexer_port=8786
prover_url="http://127.0.0.1:3001"
public_rpc="${RING_DEVNET_PUBLIC_RPC_URL:-https://api.devnet.solana.com}"
origin_cert_arn="${RING_DEVNET_ORIGIN_CERT_ARN:-}"
rpc_secret="$prefix/solana-rpc-url"
custody_secret="$prefix/ring-rpc/custody"
key_alias="alias/$prefix/ring-rpc"
# Devnet, every wrapped secret binds it in its KMS encryption context.
genesis="EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG"
signing_roles=(service storage_authority co_signer fee_payer enrollment_applier)
storage_program="rnGSTosTQ9gToNQP44THEZph8xtBtuGa5FEwzhKrWjL"
deployment="maximumPercent=100,minimumHealthyPercent=0"
sha="$(git rev-parse HEAD)"
zolana_sha="$(git -C zolana rev-parse HEAD)"

log() { printf '%s\n' "$*" >&2; }
die() { log "$*"; exit 1; }
absent() { [[ -z "$1" || "$1" == None ]]; }

aws_() {
    if ((dry_run)); then
        printf 'aws --region %s' "$region" >&3
        printf ' %q' "$@" >&3
        printf '\n' >&3
        dry_run_answer "$@"
        return 0
    fi
    aws --region "$region" "$@"
}

# Answers as an empty account for up and as a whole stack for down and status, so every call prints.
dry_run_answer() {
    case "$1 $2" in
        "sts get-caller-identity") echo 000000000000; return ;;
        "ec2 describe-vpcs") printf 'vpc-dry-run\t172.31.0.0/16\n'; return ;;
        "ec2 describe-subnets") printf 'subnet-dry-run-a\tsubnet-dry-run-b\n'; return ;;
    esac
    [[ " $* " == *" --query "* ]] || return 0
    if [[ "$command" == up ]]; then
        case "$2" in
            describe-* | list-* | get-*) echo None ;;
            *) echo "dry-run-$2" ;;
        esac
        return
    fi
    case "$1 $2" in
        "ecs describe-services") echo ACTIVE ;;
        "cloudfront list-tags-for-resource" | "kms list-resource-tags") echo 1 ;;
        "kms describe-key") printf 'arn:aws:kms:%s:000000000000:key/dry-run\tEnabled\n' "$region" ;;
        "secretsmanager get-secret-value") echo '{}' ;;
        "secretsmanager list-secrets") echo "$prefix/dry-run" ;;
        "cloudfront get-distribution-config")
            if [[ " $* " == *" --output json "* ]]; then echo '{}'; else echo dry-run-etag; fi ;;
        *) echo "dry-run-$2" ;;
    esac
}

account="$(aws_ sts get-caller-identity --query Account --output text)"
registry="$account.dkr.ecr.$region.amazonaws.com"

secret_file() { (umask 077; mktemp "$work/secret.XXXXXX"); }

# Base58 address of a KMS GetPublicKey reply, the 32 key bytes after the fixed Ed25519 SPKI header.
spki_address() {
    python3 -I - "$1" <<'PY'
import base64, sys
spec, der = sys.argv[1].split()
key = base64.b64decode(der, validate=True)
header = bytes.fromhex("302a300506032b6570032100")
if spec != "ECC_NIST_EDWARDS25519" or len(key) != 44 or not key.startswith(header):
    sys.exit("the KMS key is not an Ed25519 signing key")
key = key[len(header):]
alphabet = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
n, out = int.from_bytes(key, "big"), ""
while n:
    n, r = divmod(n, 58)
    out = alphabet[r] + out
print("1" * (len(key) - len(key.lstrip(b"\0"))) + out)
PY
}

ensure_repository() {
    local name
    name="$(aws_ ecr describe-repositories --repository-names "$1" --query 'repositories[0].repositoryName' --output text 2>/dev/null || true)"
    absent "$name" || return 0
    aws_ ecr create-repository --repository-name "$1" --image-tag-mutability IMMUTABLE --tags "$tag_spec" >/dev/null
}

# The stack's own repository, zolana's publish rules build and push into it from the checkout's images.json.
publish() {
    local checkout="$1" service="$2" tag="$3" repository="$prefix-$2" digest run
    ensure_repository "$repository"
    digest="$(aws_ ecr describe-images --repository-name "$repository" --image-ids "imageTag=$tag" \
        --query 'imageDetails[0].imageDigest' --output text 2>/dev/null || true)"
    absent "$digest" || return 0
    run=(env "ZOLANA_ECR_REGISTRY=$registry" "AWS_REGION=$region" "$root/zolana/tools/publish-image.sh" "$service"
        --repository "$repository" --tag "$tag" --push)
    if ((dry_run)); then
        printf 'cd %s && %s\n' "$checkout" "${run[*]}" >&3
        return 0
    fi
    (cd "$checkout" && "${run[@]}" >&2)
}

vpc_and_subnets() {
    local vpc cidr subnets
    if [[ -n "${RING_DEVNET_VPC:-}" ]]; then
        read -r vpc cidr <<< "$(aws_ ec2 describe-vpcs --vpc-ids "$RING_DEVNET_VPC" --query 'Vpcs[0].[VpcId,CidrBlock]' --output text)"
    else
        read -r vpc cidr <<< "$(aws_ ec2 describe-vpcs --filters Name=is-default,Values=true --query 'Vpcs[0].[VpcId,CidrBlock]' --output text)"
    fi
    absent "$vpc" && die "no default VPC, set RING_DEVNET_VPC"
    subnets="$(aws_ ec2 describe-subnets --filters "Name=vpc-id,Values=$vpc" "Name=map-public-ip-on-launch,Values=true" \
        --query 'Subnets[].SubnetId' --output text | tr '\t' ',')"
    absent "$subnets" && die "VPC $vpc has no public subnets"
    echo "$vpc" "$cidr" "$subnets"
}

security_group_id() {
    aws_ ec2 describe-security-groups --filters "Name=group-name,Values=$1" "Name=vpc-id,Values=$2" \
        --query 'SecurityGroups[0].GroupId' --output text
}

# Targets answer the balancer, whose nodes reach them from the VPC range alone.
ensure_security_group() {
    local vpc="$1" cidr="$2" id port
    id="$(security_group_id "$security_group" "$vpc")"
    absent "$id" || { echo "$id"; return; }
    id="$(aws_ ec2 create-security-group --group-name "$security_group" --description "$prefix tasks" --vpc-id "$vpc" \
        --tag-specifications "ResourceType=security-group,Tags=[{$tag_spec}]" --query GroupId --output text)"
    for port in $photon_port $ring_rpc_port $ring_indexer_port; do
        aws_ ec2 authorize-security-group-ingress --group-id "$id" --protocol tcp --port "$port" --cidr "$cidr" >/dev/null
    done
    echo "$id"
}

ensure_db_security_group() {
    local vpc="$1" task_sg="$2" id
    id="$(security_group_id "$db_security_group" "$vpc")"
    absent "$id" || { echo "$id"; return; }
    id="$(aws_ ec2 create-security-group --group-name "$db_security_group" --description "$prefix databases" --vpc-id "$vpc" \
        --tag-specifications "ResourceType=security-group,Tags=[{$tag_spec}]" --query GroupId --output text)"
    aws_ ec2 authorize-security-group-ingress --group-id "$id" --protocol tcp --port 5432 --source-group "$task_sg" >/dev/null
    echo "$id"
}

target_group_arn() {
    aws_ elbv2 describe-target-groups --names "$prefix-$1" --query 'TargetGroups[0].TargetGroupArn' --output text 2>/dev/null || true
}

ensure_load_balancer() {
    local vpc="$1" subnets="$2" arn service port health group
    arn="$(aws_ elbv2 describe-load-balancers --names "$load_balancer" --query 'LoadBalancers[0].LoadBalancerArn' --output text 2>/dev/null || true)"
    if absent "$arn"; then
        local subnet_list
        IFS=, read -r -a subnet_list <<< "$subnets"
        arn="$(aws_ elbv2 create-load-balancer --name "$load_balancer" --type network --scheme internet-facing \
            --subnets "${subnet_list[@]}" --tags "$tag_spec" --query 'LoadBalancers[0].LoadBalancerArn' --output text)"
    fi
    # Each task runs in one zone, a node of another zone with no target accepts and never answers.
    aws_ elbv2 modify-load-balancer-attributes --load-balancer-arn "$arn" \
        --attributes Key=load_balancing.cross_zone.enabled,Value=true >/dev/null
    for service in photon ring-indexer ring-rpc; do
        case "$service" in
            photon) port=$photon_port; health=/liveness ;;
            ring-indexer) port=$ring_indexer_port; health=/health ;;
            ring-rpc) port=$ring_rpc_port; health=/health ;;
        esac
        group="$(target_group_arn "$service")"
        if absent "$group"; then
            group="$(aws_ elbv2 create-target-group --name "$prefix-$service" --protocol TCP --port "$port" --vpc-id "$vpc" \
                --target-type ip --health-check-protocol HTTP --health-check-path "$health" --tags "$tag_spec" \
                --query 'TargetGroups[0].TargetGroupArn' --output text)"
        fi
        # The source stays a balancer node inside the VPC range the task security group admits.
        aws_ elbv2 modify-target-group-attributes --target-group-arn "$group" --attributes \
            Key=preserve_client_ip.enabled,Value=false Key=deregistration_delay.timeout_seconds,Value=30 >/dev/null
        if absent "$(aws_ elbv2 describe-listeners --load-balancer-arn "$arn" --query "Listeners[?Port==\`$port\`].ListenerArn | [0]" --output text)"; then
            if [[ -n "$origin_cert_arn" ]]; then
                aws_ elbv2 create-listener --load-balancer-arn "$arn" --protocol TLS --port "$port" \
                    --certificates "CertificateArn=$origin_cert_arn" \
                    --default-actions "Type=forward,TargetGroupArn=$group" --tags "$tag_spec" >/dev/null
            else
                aws_ elbv2 create-listener --load-balancer-arn "$arn" --protocol TCP --port "$port" \
                    --default-actions "Type=forward,TargetGroupArn=$group" --tags "$tag_spec" >/dev/null
            fi
        fi
    done
    aws_ elbv2 describe-load-balancers --load-balancer-arns "$arn" --query 'LoadBalancers[0].DNSName' --output text
}

# A distribution belongs to the stack only under its Comment and the stack tag.
own_distribution() {
    local name="$1" id tagged
    for id in $(aws_ cloudfront list-distributions --query "DistributionList.Items[?Comment=='$name'].Id" --output text); do
        [[ "$id" != None && "$id" != "$protected_distribution" ]] || continue
        tagged="$(aws_ cloudfront list-tags-for-resource --resource "arn:aws:cloudfront::$account:distribution/$id" \
            --query "Tags.Items[?Key=='$prefix'].Value | [0]" --output text)"
        [[ "$tagged" == 1 ]] && { echo "$id"; return; }
    done
    return 0
}

# CloudFront gives HTTPS on an amazon hostname with Amazon's certificate, no domain to validate.
ensure_distribution() {
    local name="$1" origin="$2" origin_port="$3" id origin_policy="http-only" http_port="$origin_port" https_port=443
    if [[ -n "$origin_cert_arn" ]]; then
        origin="${RING_DEVNET_ORIGIN_DOMAIN:-$origin}"
        origin_policy="https-only"
        http_port=80
        https_port="$origin_port"
    fi
    id="$(own_distribution "$name")"
    if absent "$id"; then
        id="$(aws_ cloudfront create-distribution-with-tags --distribution-config-with-tags "$(jq -n \
            --arg name "$name" --arg reference "$name-$(date +%s)" --arg origin "$origin" \
            --argjson http_port "$http_port" --argjson https_port "$https_port" \
            --arg origin_policy "$origin_policy" --arg key "$prefix" '{
            DistributionConfig: {CallerReference: $reference, Comment: $name, Enabled: true, HttpVersion: "http2",
                PriceClass: "PriceClass_100",
                Origins: {Quantity: 1, Items: [{Id: "origin", DomainName: $origin,
                    CustomOriginConfig: {HTTPPort: $http_port, HTTPSPort: $https_port, OriginProtocolPolicy: $origin_policy,
                        OriginReadTimeout: 60, OriginKeepaliveTimeout: 5,
                        OriginSslProtocols: {Quantity: 1, Items: ["TLSv1.2"]}}}]},
                DefaultCacheBehavior: {TargetOriginId: "origin", ViewerProtocolPolicy: "redirect-to-https",
                    AllowedMethods: {Quantity: 7, Items: ["GET", "HEAD", "OPTIONS", "PUT", "POST", "PATCH", "DELETE"],
                        CachedMethods: {Quantity: 2, Items: ["GET", "HEAD"]}},
                    CachePolicyId: "4135ea2d-6df8-44a3-9df3-4b5a84be39ad",
                    OriginRequestPolicyId: "b689b0a8-53d0-40ab-baf2-68738e2966ac", Compress: true}},
            Tags: {Items: [{Key: $key, Value: "1"}]}}')" --query 'Distribution.Id' --output text)"
    fi
    aws_ cloudfront get-distribution --id "$id" --query 'Distribution.DomainName' --output text
}

distribution_url() {
    local id host
    id="$(own_distribution "$1")"
    absent "$id" && { echo "-"; return; }
    host="$(aws_ cloudfront get-distribution --id "$id" --query 'Distribution.DomainName' --output text)"
    absent "$host" && echo "-" || echo "https://$host"
}

secret_arn() {
    aws_ secretsmanager describe-secret --secret-id "$1" --query ARN --output text 2>/dev/null || true
}

# Sets key_arn to the enabled key under the stack alias of $1, None while the alias is absent.
existing_key() {
    local state
    read -r key_arn state <<< "$(aws_ kms describe-key --key-id "$key_alias/$1" \
        --query 'KeyMetadata.[Arn,KeyState]' --output text 2>/dev/null || true)"
    key_arn="${key_arn:-None}"
    absent "$key_arn" || [[ "$state" == Enabled ]] || die "KMS key $key_alias/$1 is $state"
}

# Sets key_arn, a key is never rotated because the manifest pins its ARN.
ensure_kms_key() {
    local name="$1" spec="$2" usage="$3"
    existing_key "$name"
    absent "$key_arn" || return 0
    key_arn="$(aws_ kms create-key --key-spec "$spec" --key-usage "$usage" --description "$prefix ring-rpc $name" \
        --tags "TagKey=$prefix,TagValue=1" --query KeyMetadata.Arn --output text)"
    ((dry_run)) && key_arn="arn:aws:kms:$region:$account:key/dry-run-$name"
    aws_ kms create-alias --alias-name "$key_alias/$name" --target-key-id "$key_arn"
}

# Empty when the secret is absent or unreadable.
custody_manifest() {
    local value
    value="$(aws_ secretsmanager get-secret-value --secret-id "$custody_secret" --query SecretString --output text 2>/dev/null || true)"
    absent "$value" || echo "$value"
}

# Sets custody, the manifest is written once because a new wrapped root orphans every ring.
ensure_custody() {
    local role name reply address wrapping ciphertext file
    custody="$(custody_manifest)"
    if [[ -n "$custody" ]]; then
        for role in wrapping "${signing_roles[@]}"; do
            name="${role//_/-}"
            existing_key "$name"
            jq -e --arg role "$role" --arg arn "$key_arn" 'if $role == "wrapping"
                then .auditor.key_arn == $arn and .grant_rows.key_arn == $arn else .[$role].key_arn == $arn end' \
                <<< "$custody" >/dev/null || die "$custody_secret pins a key other than $key_alias/$name"
        done
        return 0
    fi
    custody='{}'
    for role in "${signing_roles[@]}"; do
        ensure_kms_key "${role//_/-}" ECC_NIST_EDWARDS25519 SIGN_VERIFY
        reply="$(aws_ kms get-public-key --key-id "$key_arn" --query '[KeySpec,PublicKey]' --output text)"
        if ((dry_run)); then address="${key_arn##*/}-address"; else address="$(spki_address "$reply")"; fi
        custody="$(jq -c --arg role "$role" --arg key_arn "$key_arn" --arg address "$address" \
            '.[$role] = {$key_arn, $address}' <<< "$custody")"
    done
    ensure_kms_key wrapping SYMMETRIC_DEFAULT ENCRYPT_DECRYPT
    wrapping="$key_arn"
    for role in auditor-root grant-rows; do
        # Ring RPC opens the secret only under an identical context.
        ciphertext="$(aws_ kms generate-data-key-without-plaintext --key-id "$wrapping" --key-spec AES_256 \
            --encryption-context "$(jq -cn --arg deployment "$prefix" --arg genesis "$genesis" --arg purpose "$role" \
                '{application: "zolana-custom-rings-v1", $deployment, $genesis, $purpose}')" \
            --query CiphertextBlob --output text)"
        custody="$(jq -c --arg role "${role%-root}" --arg key_arn "$wrapping" --arg ciphertext "$ciphertext" \
            '.[$role | sub("-"; "_")] = {$key_arn, $ciphertext}' <<< "$custody")"
    done
    file="$(secret_file)"
    printf '%s' "$custody" > "$file"
    ((dry_run)) && printf '# %s holds %s\n' "$custody_secret" "$custody" >&3
    aws_ secretsmanager create-secret --name "$custody_secret" --secret-string "file://$file" --tags "$tag_spec" >/dev/null
}

# The URL can carry a provider key, it moves through a 077 file and never a command line.
ensure_rpc_secret() {
    local file
    file="$(secret_file)"
    printf '%s' "${RING_DEVNET_RPC_URL:-https://api.devnet.solana.com}" > "$file"
    if absent "$(secret_arn "$rpc_secret")"; then
        aws_ secretsmanager create-secret --name "$rpc_secret" --secret-string "file://$file" --tags "$tag_spec" >/dev/null
    elif [[ -n "${RING_DEVNET_RPC_URL:-}" ]]; then
        aws_ secretsmanager put-secret-value --secret-id "$rpc_secret" --secret-string "file://$file" >/dev/null
    fi
    secret_arn "$rpc_secret"
}

ensure_password_secret() {
    local name="$prefix/$1/database-password" file
    if absent "$(secret_arn "$name")"; then
        file="$(secret_file)"
        openssl rand -hex 24 | tr -d '\n' > "$file"
        aws_ secretsmanager create-secret --name "$name" --secret-string "file://$file" --tags "$tag_spec" >/dev/null
    fi
    secret_arn "$name"
}

db_instance() {
    aws_ rds describe-db-instances --db-instance-identifier "$prefix-$1" \
        --query 'DBInstances[0].DBInstanceIdentifier' --output text 2>/dev/null || true
}

# Photon and the ring indexer rebuild from the chain, the Ring RPC store holds grants and relays and keeps backups.
create_database() {
    local service="$1" user="$2" password_arn="$3" db_sg="$4" backups=0 file
    absent "$(db_instance "$service")" || return 0
    [[ "$service" != ring-rpc ]] || backups=7
    file="$(secret_file)"
    aws_ secretsmanager get-secret-value --secret-id "$password_arn" --query SecretString --output text | tr -d '\n' > "$file"
    aws_ rds create-db-instance --db-instance-identifier "$prefix-$service" --db-instance-class "$db_class" \
        --engine postgres --master-username "$user" --master-user-password "file://$file" --db-name "$user" \
        --allocated-storage 20 --max-allocated-storage 200 --storage-type gp3 --storage-encrypted \
        --no-publicly-accessible --no-multi-az \
        --backup-retention-period "$backups" --db-subnet-group-name "$db_subnet_group" \
        --vpc-security-group-ids "$db_sg" --tags "$tag_spec" >/dev/null
}

database_host() {
    aws_ rds wait db-instance-available --db-instance-identifier "$prefix-$1"
    aws_ rds describe-db-instances --db-instance-identifier "$prefix-$1" --query 'DBInstances[0].Endpoint.Address' --output text
}

# Sets created_role, the trust policy in $2 applies only to a new role.
ensure_role() {
    local name="$1" trust="$2"
    created_role="$(aws_ iam get-role --role-name "$name" --query Role.Arn --output text 2>/dev/null || true)"
    absent "$created_role" || return 0
    created_role="$(aws_ iam create-role --role-name "$name" --tags "$tag_spec" --assume-role-policy-document "$trust" \
        --query Role.Arn --output text)"
    ((dry_run)) || { log "waiting for $name to propagate"; sleep 15; }
}

ensure_roles() {
    local trust='{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Principal":{"Service":"ecs-tasks.amazonaws.com"},"Action":"sts:AssumeRole"}]}'
    ensure_role "$role" "$trust"
    role_arn="$created_role"
    aws_ iam attach-role-policy --role-name "$role" \
        --policy-arn arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy
    # Only the secrets the tasks read, any other secret under the prefix stays out of reach.
    aws_ iam put-role-policy --role-name "$role" --policy-name secrets --policy-document "$(jq -cn \
        --arg base "arn:aws:secretsmanager:$region:$account:secret:$prefix" '{
        Version: "2012-10-17",
        Statement: [{Effect: "Allow", Action: "secretsmanager:GetSecretValue",
            Resource: [$base + ("/ring-rpc/custody", "/solana-rpc-url", "/photon/database-password",
                "/ring-indexer/database-password", "/ring-rpc/database-password") + "-??????"]}]}')"

    ensure_role "$task_role" "$(jq -c --arg account "$account" --arg source "arn:aws:ecs:$region:$account:*" \
        '.Statement[0].Condition = {StringEquals: {"aws:SourceAccount": $account}, ArnLike: {"aws:SourceArn": $source}}' \
        <<< "$trust")"
    task_role_arn="$created_role"
    aws_ iam put-role-policy --role-name "$task_role" --policy-name custody --policy-document "$(jq -c \
        --arg deployment "$prefix" --arg genesis "$genesis" '
        ([.service, .storage_authority, .co_signer, .fee_payer, .enrollment_applier] | map(.key_arn)) as $signers
        | ([.auditor, .grant_rows] | map(.key_arn) | unique) as $wrapping
        | {Version: "2012-10-17", Statement: [
            {Effect: "Allow", Action: "kms:GetPublicKey", Resource: $signers},
            {Effect: "Allow", Action: "kms:Sign", Resource: $signers,
             Condition: {StringEquals: {"kms:SigningAlgorithm": "ED25519_SHA_512", "kms:MessageType": "RAW"}}},
            {Effect: "Allow", Action: "kms:Decrypt", Resource: $wrapping,
             Condition: {StringEquals: {"kms:EncryptionContext:application": "zolana-custom-rings-v1",
                 "kms:EncryptionContext:deployment": $deployment, "kms:EncryptionContext:genesis": $genesis,
                 "kms:EncryptionContext:purpose": ["auditor-root", "grant-rows"]}}}]}' <<< "$custody")"
}

# A JSON object of the alternating key and value arguments.
json_object() {
    jq -n '[range(0; $ARGS.positional | length; 2) as $i | {($ARGS.positional[$i]): $ARGS.positional[$i + 1]}] | add // {}' \
        --args "$@"
}

# One container whose shell joins its secrets into files and URLs before it execs the service.
shell_container() {
    local name="$1" image="$2" script="$3" secrets="$4" environment="$5" port="${6:-0}"
    jq -n --arg name "$name" --arg image "$image" --arg script "$script" --argjson secrets "$secrets" \
        --argjson environment "$environment" --argjson port "$port" --arg group "$log_group" --arg region "$region" '
        {name: $name, image: $image, essential: true, entryPoint: ["/bin/sh", "-c"], command: [$script],
         secrets: ($secrets | to_entries | map({name: .key, valueFrom: .value})),
         environment: ($environment | to_entries | map({name: .key, value: .value})),
         logConfiguration: {logDriver: "awslogs",
             options: {"awslogs-group": $group, "awslogs-region": $region, "awslogs-stream-prefix": $name}}}
        + (if $port == 0 then {} else {portMappings: [{containerPort: $port, protocol: "tcp"}]} end)'
}

register_task() {
    local family="$1" cpu="$2" memory="$3" containers="$4"
    shift 4
    aws_ ecs register-task-definition --family "$prefix-$family" --tags "$ecs_tag_spec" \
        --requires-compatibilities FARGATE --network-mode awsvpc --cpu "$cpu" --memory "$memory" \
        --execution-role-arn "$role_arn" --runtime-platform cpuArchitecture=X86_64,operatingSystemFamily=LINUX \
        "$@" --container-definitions "$containers" --query 'taskDefinition.taskDefinitionArn' --output text
}

# shellcheck disable=SC2016
photon_script='set -eu
db="postgres://photon:$DB_PASSWORD@$DB_HOST:5432/photon?sslmode=require"
unset DB_PASSWORD
set -- photon --port '"$photon_port"' --rpc-url "$SOLANA_RPC_URL" --db-url "$db" --max-db-conn 40 --logging-format json
if [ -n "$PHOTON_START_SLOT" ]; then set -- "$@" --start-slot "$PHOTON_START_SLOT"; fi
if [ -n "$PHOTON_BLOCK_FETCHES" ]; then set -- "$@" --max-concurrent-block-fetches "$PHOTON_BLOCK_FETCHES"; fi
echo "photon start_slot=$PHOTON_START_SLOT block_fetches=$PHOTON_BLOCK_FETCHES"
exec "$@"'

# shellcheck disable=SC2016
migration_script='set -eu
export DATABASE_URL="postgres://photon:$DB_PASSWORD@$DB_HOST:5432/photon?sslmode=require"
unset DB_PASSWORD
exec photon-migration up'

# shellcheck disable=SC2016
ring_indexer_script='set -eu
export RING_INDEXER_DB_URL="postgres://ring_indexer:$DB_PASSWORD@$DB_HOST:5432/ring_indexer?sslmode=require"
unset DB_PASSWORD
exec ring-indexer --rpc-url "$SOLANA_RPC_URL" --photon-url "$PHOTON_URL" --bind 0.0.0.0 --port '"$ring_indexer_port"' --start-slot "$START_SLOT"'

# shellcheck disable=SC2016
ring_rpc_script='set -eu
export RING_RPC_STORE_URL="postgres://ring_rpc:$DB_PASSWORD@$DB_HOST:5432/ring_rpc?sslmode=require"
unset DB_PASSWORD
exec ring-rpc serve'

# Postgres has no table before the migration, and Photon migrates only its own SQLite.
run_migration() {
    local image="$1" password_arn="$2" host="$3" subnets="$4" sg="$5" td task exit_code
    td="$(register_task photon-migration 512 1024 "[$(shell_container photon-migration "$image" "$migration_script" \
        "$(json_object DB_PASSWORD "$password_arn")" "$(json_object DB_HOST "$host" RUST_LOG info)")]")"
    task="$(aws_ ecs run-task --cluster "$cluster" --task-definition "$td" --launch-type FARGATE \
        --network-configuration "awsvpcConfiguration={subnets=[$subnets],securityGroups=[$sg],assignPublicIp=ENABLED}" \
        --query 'tasks[0].taskArn' --output text)"
    ((dry_run)) || ! absent "$task" || die "the migration task did not start"
    aws_ ecs wait tasks-stopped --cluster "$cluster" --tasks "$task"
    exit_code="$(aws_ ecs describe-tasks --cluster "$cluster" --tasks "$task" \
        --query 'tasks[0].containers[0].exitCode' --output text)"
    ((dry_run)) || [[ "$exit_code" == 0 ]] || die "the migration task exited $exit_code"
}

# Each service runs one task and stops it before the next starts, a store and an index take one writer.
ensure_service() {
    local service="$1" task_definition="$2" subnets="$3" sg="$4" port="$5" status
    status="$(aws_ ecs describe-services --cluster "$cluster" --services "$prefix-$service" \
        --query 'services[0].status' --output text 2>/dev/null || true)"
    if [[ "$status" == ACTIVE ]]; then
        aws_ ecs update-service --cluster "$cluster" --service "$prefix-$service" --task-definition "$task_definition" \
            --deployment-configuration "$deployment" --force-new-deployment >/dev/null
    else
        aws_ ecs create-service --cluster "$cluster" --service-name "$prefix-$service" --task-definition "$task_definition" \
            --desired-count 1 --launch-type FARGATE --tags "$ecs_tag_spec" --health-check-grace-period-seconds 300 \
            --deployment-configuration "$deployment" \
            --load-balancers "targetGroupArn=$(target_group_arn "$service"),containerName=$service,containerPort=$port" \
            --network-configuration "awsvpcConfiguration={subnets=[$subnets],securityGroups=[$sg],assignPublicIp=ENABLED}" >/dev/null
    fi
}

positive() {
    [[ "$2" =~ ^[1-9][0-9]*$ ]] || die "$1 must be a positive integer, got '$2'"
    echo "$2"
}

# Mirrors the Ring RPC rule, every origin host is the RP id or, for a domain RP id, one of its subdomains.
check_origins() {
    local origins="$1" rp="$2" origin host list
    [[ "$origins" != "*" ]] || return 0
    [[ -n "$rp" ]] || die "set RING_DEVNET_WEBAUTHN_RP_ID"
    IFS=, read -r -a list <<< "$origins"
    for origin in "${list[@]}"; do
        host="${origin#*://}"
        host="${host%%[:/]*}"
        [[ "$host" == "$rp" ]] && continue
        if [[ "$rp" == localhost || "$rp" =~ ^[0-9.]+$ || "$host" != *".$rp" ]]; then
            die "origin $origin sits outside the WebAuthn RP id $rp, the Ring RPC would refuse to start"
        fi
    done
}

check_devnet_origin() {
    if [[ -n "$origin_cert_arn" ]]; then
        [[ -n "${RING_DEVNET_ORIGIN_DOMAIN:-}" ]] \
            || die "RING_DEVNET_ORIGIN_CERT_ARN requires RING_DEVNET_ORIGIN_DOMAIN because ACM certificates cannot match *.elb.amazonaws.com and require a custom origin domain alias"
        return 0
    fi
    local rpc_url="${RING_DEVNET_RPC_URL:-$public_rpc}" candidate lowered observed
    for candidate in "$rpc_url" "$public_rpc"; do
        lowered="$(printf '%s' "$candidate" | tr '[:upper:]' '[:lower:]')"
        if [[ "$lowered" == *mainnet* ]]; then
            die "plaintext CloudFront origin is refused off devnet, set RING_DEVNET_ORIGIN_CERT_ARN"
        fi
    done
    ((dry_run)) && return 0
    observed="$(curl -sS --max-time 15 -H 'Content-Type: application/json' \
        -d '{"jsonrpc":"2.0","id":1,"method":"getGenesisHash"}' "$rpc_url" \
        | jq -r '.result // empty' 2>/dev/null || true)"
    [[ "$observed" == "$genesis" ]] \
        || die "plaintext CloudFront origin requires devnet genesis $genesis, got '${observed:-unreadable}'"
}

up() {
    check_devnet_origin
    local origins="${RING_DEVNET_ALLOW_ORIGINS:-http://localhost:3000}" rp first
    first="${origins%%,*}"
    first="${first#*://}"
    rp="${RING_DEVNET_WEBAUTHN_RP_ID:-${first%%[:/]*}}"
    check_origins "$origins" "$rp"
    local budget signer_budget enrollment_budget ring_budget max_rings grants="${RING_DEVNET_READER_GRANTS:-database}"
    budget="$(positive RING_DEVNET_RELAY_BUDGET_LAMPORTS "${RING_DEVNET_RELAY_BUDGET_LAMPORTS:-10000000000}")"
    signer_budget="$(positive RING_DEVNET_RELAY_SIGNER_BUDGET_LAMPORTS "${RING_DEVNET_RELAY_SIGNER_BUDGET_LAMPORTS:-100000000}")"
    enrollment_budget="$(positive RING_DEVNET_ENROLLMENT_BUDGET_LAMPORTS "${RING_DEVNET_ENROLLMENT_BUDGET_LAMPORTS:-1000000000}")"
    ring_budget="$(positive RING_DEVNET_ENROLLMENT_RING_BUDGET_LAMPORTS "${RING_DEVNET_ENROLLMENT_RING_BUDGET_LAMPORTS:-20000000}")"
    max_rings="$(positive RING_DEVNET_ENROLLMENT_MAX_RINGS "${RING_DEVNET_ENROLLMENT_MAX_RINGS:-64}")"
    [[ "$grants" == tree || "$grants" == database ]] || die "RING_DEVNET_READER_GRANTS is tree or database"
    if absent "$(db_instance photon)" && [[ -z "${RING_DEVNET_PHOTON_START_SLOT:-}" ]]; then
        die "a new Photon database needs RING_DEVNET_PHOTON_START_SLOT, without it Photon backfills devnet from slot 319998226"
    fi
    local ring_rpc_tag="${RING_DEVNET_RING_RPC_TAG:-ring-rpc-${sha:0:12}}"
    local ring_indexer_tag="${RING_DEVNET_RING_INDEXER_TAG:-ring-indexer-${sha:0:12}}"
    local prover_tag="${RING_DEVNET_PROVER_TAG:-ring-prover-${sha:0:12}}"
    local photon_tag="${RING_DEVNET_PHOTON_TAG:-photon-${zolana_sha:0:12}}"

    log "== images"
    publish "$root" ring-rpc "$ring_rpc_tag"
    publish "$root" ring-indexer "$ring_indexer_tag"
    publish "$root" ring-prover "$prover_tag"
    publish "$root/zolana" photon "$photon_tag"

    log "== network, load balancer, logs, cluster"
    local network vpc cidr subnets sg db_sg dns
    network="$(vpc_and_subnets)"
    read -r vpc cidr subnets <<< "$network"
    sg="$(ensure_security_group "$vpc" "$cidr")"
    db_sg="$(ensure_db_security_group "$vpc" "$sg")"
    dns="$(ensure_load_balancer "$vpc" "$subnets")"
    absent "$(aws_ logs describe-log-groups --log-group-name-prefix "$log_group" \
        --query "logGroups[?logGroupName=='$log_group'].logGroupName | [0]" --output text)" \
        && aws_ logs create-log-group --log-group-name "$log_group" --tags "$prefix=1"
    absent "$(aws_ ecs describe-clusters --clusters "$cluster" --query "clusters[?status=='ACTIVE'].clusterName | [0]" --output text)" \
        && aws_ ecs create-cluster --cluster-name "$cluster" --tags "$ecs_tag_spec" >/dev/null

    log "== custody and roles"
    ensure_custody
    ensure_roles

    log "== secrets"
    local rpc_arn custody_arn
    rpc_arn="$(ensure_rpc_secret)"
    custody_arn="$(secret_arn "$custody_secret")"
    local photon_password ring_indexer_password ring_rpc_password
    photon_password="$(ensure_password_secret photon)"
    ring_indexer_password="$(ensure_password_secret ring-indexer)"
    ring_rpc_password="$(ensure_password_secret ring-rpc)"

    log "== databases"
    if absent "$(aws_ rds describe-db-subnet-groups --db-subnet-group-name "$db_subnet_group" \
        --query 'DBSubnetGroups[0].DBSubnetGroupName' --output text 2>/dev/null || true)"; then
        local subnet_list
        IFS=, read -r -a subnet_list <<< "$subnets"
        aws_ rds create-db-subnet-group --db-subnet-group-name "$db_subnet_group" \
            --db-subnet-group-description "$prefix" --subnet-ids "${subnet_list[@]}" --tags "$tag_spec" >/dev/null
    fi
    create_database photon photon "$photon_password" "$db_sg"
    create_database ring-indexer ring_indexer "$ring_indexer_password" "$db_sg"
    create_database ring-rpc ring_rpc "$ring_rpc_password" "$db_sg"
    log "waiting for the databases"
    local photon_db ring_indexer_db ring_rpc_db
    photon_db="$(database_host photon)"
    ring_indexer_db="$(database_host ring-indexer)"
    ring_rpc_db="$(database_host ring-rpc)"

    # Tasks reach each other through the balancer.
    local photon_url="http://$dns:$photon_port" ring_indexer_url="http://$dns:$ring_indexer_port"
    if [[ -n "$origin_cert_arn" ]]; then
        photon_url="https://${RING_DEVNET_ORIGIN_DOMAIN}:$photon_port"
        ring_indexer_url="https://${RING_DEVNET_ORIGIN_DOMAIN}:$ring_indexer_port"
    fi

    log "== migration"
    run_migration "$registry/$prefix-photon:$photon_tag" "$photon_password" "$photon_db" "$subnets" "$sg"

    log "== services"
    local photon_td ring_indexer_td ring_rpc_td prover ring_rpc
    photon_td="$(register_task photon "${RING_DEVNET_PHOTON_CPU:-2048}" "${RING_DEVNET_PHOTON_MEMORY:-8192}" \
        "[$(shell_container photon "$registry/$prefix-photon:$photon_tag" "$photon_script" \
            "$(json_object DB_PASSWORD "$photon_password" SOLANA_RPC_URL "$rpc_arn")" \
            "$(json_object DB_HOST "$photon_db" RUST_LOG info PHOTON_START_SLOT "${RING_DEVNET_PHOTON_START_SLOT:-}" \
                PHOTON_BLOCK_FETCHES "${RING_DEVNET_PHOTON_BLOCK_FETCHES:-}")" "$photon_port")]")"
    ring_indexer_td="$(register_task ring-indexer 1024 2048 \
        "[$(shell_container ring-indexer "$registry/$prefix-ring-indexer:$ring_indexer_tag" "$ring_indexer_script" \
            "$(json_object DB_PASSWORD "$ring_indexer_password" SOLANA_RPC_URL "$rpc_arn")" \
            "$(json_object DB_HOST "$ring_indexer_db" PHOTON_URL "$photon_url" RUST_LOG info \
                START_SLOT "${RING_DEVNET_RING_INDEXER_START_SLOT:-latest}")" "$ring_indexer_port")]")"
    # A core dump would write the memory of either container, opened secrets included, to the task disk.
    local no_core='{"ulimits": [{"name": "core", "softLimit": 0, "hardLimit": 0}]}'
    # The prover binds loopback, the Rust prover client accepts plain HTTP only there.
    prover="$(jq -n --arg image "$registry/$prefix-ring-prover:$prover_tag" --arg indexer "$ring_indexer_url" \
        --arg prover "${prover_url#http://}" --arg group "$log_group" --arg region "$region" --argjson no_core "$no_core" '
        {name: "prover", image: $image, essential: true,
         command: ["start", "--require-optimized-build", "--keys-dir", "/proving-keys/", "--auto-download=true",
                   "--prover-address", $prover, "--metrics-address", "127.0.0.1:9998", "--indexer-url", $indexer],
         logConfiguration: {logDriver: "awslogs",
             options: {"awslogs-group": $group, "awslogs-region": $region, "awslogs-stream-prefix": "prover"}}}
        + $no_core')"
    # Every client arrives from a balancer node address and shares its rate buckets, ten times the defaults.
    ring_rpc="$(shell_container ring-rpc "$registry/$prefix-ring-rpc:$ring_rpc_tag" "$ring_rpc_script" \
        "$(json_object RING_RPC_CUSTODY_JSON "$custody_arn" DB_PASSWORD "$ring_rpc_password" \
            RING_RPC_SOLANA_RPC_URL "$rpc_arn")" \
        "$(json_object DB_HOST "$ring_rpc_db" AWS_REGION "$region" RING_RPC_DEPLOYMENT "$prefix" \
            RING_RPC_BIND 0.0.0.0 RING_RPC_INSECURE_PUBLIC_BIND true RING_RPC_PORT "$ring_rpc_port" \
            RING_RPC_INDEXER_URL "$ring_indexer_url" RING_RPC_PROVER_URL "$prover_url" \
            RING_RPC_REPLICA_ID "$prefix-ring-rpc" \
            RING_RPC_CO_SIGNER_APPROVE "${RING_DEVNET_CO_SIGNER_APPROVE:-outflow,withdrawal,transfers,deposits,withdrawals}" \
            RING_RPC_RELAY_BUDGET_LAMPORTS "$budget" RING_RPC_RELAY_SIGNER_BUDGET_LAMPORTS "$signer_budget" \
            RING_RPC_READER_GRANTS "$grants" \
            RING_RPC_ENROLLMENT_BUDGET_LAMPORTS "$enrollment_budget" \
            RING_RPC_ENROLLMENT_RING_BUDGET_LAMPORTS "$ring_budget" RING_RPC_ENROLLMENT_MAX_RINGS "$max_rings" \
            RING_RPC_ALLOW_ORIGINS "$origins" RING_RPC_WEBAUTHN_RP_ID "$rp" \
            RING_RPC_PUBLIC_READS_PER_MINUTE 6000 RING_RPC_PUBLIC_READ_BURST 600 \
            RING_RPC_SIGNED_READS_PER_MINUTE 1200 RING_RPC_SIGNED_READ_BURST 200 \
            RING_RPC_SIGNED_WRITES_PER_MINUTE 300 RING_RPC_SIGNED_WRITE_BURST 100)" "$ring_rpc_port" \
        | jq --argjson no_core "$no_core" '. + {dependsOn: [{containerName: "prover", condition: "START"}]} + $no_core')"
    ring_rpc_td="$(register_task ring-rpc "${RING_DEVNET_RING_RPC_CPU:-4096}" "${RING_DEVNET_RING_RPC_MEMORY:-16384}" \
        "[$prover,$ring_rpc]" --ephemeral-storage sizeInGiB=40 --task-role-arn "$task_role_arn")"
    ensure_service photon "$photon_td" "$subnets" "$sg" "$photon_port"
    ensure_service ring-indexer "$ring_indexer_td" "$subnets" "$sg" "$ring_indexer_port"
    ensure_service ring-rpc "$ring_rpc_td" "$subnets" "$sg" "$ring_rpc_port"

    log "== https"
    ensure_distribution "$prefix-photon" "$dns" "$photon_port" >/dev/null
    ensure_distribution "$prefix-ring-indexer" "$dns" "$ring_indexer_port" >/dev/null
    ensure_distribution "$prefix-ring-rpc" "$dns" "$ring_rpc_port" >/dev/null
    log "waiting for the services to stabilize"
    aws_ ecs wait services-stable --cluster "$cluster" --services "$prefix-photon" "$prefix-ring-indexer" "$prefix-ring-rpc"
    status
}

probe() {
    local name="$1" url="$2" code
    if ((dry_run)) || [[ "$url" == - ]]; then
        printf '%-18s %s\n' "$name" "-"
        return
    fi
    code="$(curl -s -o /dev/null -w '%{http_code}' --max-time 15 "$url" || true)"
    printf '%-18s %s\n' "$name" "$([[ "$code" == 200 ]] && echo ready || echo "not ready ($code)")"
}

address() {
    local value
    value="$(jq -r --arg role "$2" '.[$role].address // empty' <<< "$1" 2>/dev/null || true)"
    echo "${value:--}"
}

# A task reads the manifest only at start, a later manifest differs until the next task.
served_keys() {
    local url="$1" manifest="$2" served
    if ((dry_run)) || [[ "$url" == - || -z "$manifest" ]]; then
        echo -
        return
    fi
    served="$(curl -s --max-time 15 -H 'Content-Type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"health"}' \
        "$url/health" | jq -c '.result | [.servicePubkey, .storageAuthorityPubkey, .coSignerPubkey,
            .feePayerPubkey, .enrollmentWriterPubkey]' 2>/dev/null || true)"
    if [[ -z "$served" ]]; then
        echo "not ready"
    elif [[ "$served" == "$(jq -c '[.service, .storage_authority, .co_signer, .fee_payer, .enrollment_applier]
        | map(.address)' <<< "$manifest")" ]]; then
        echo "match custody"
    else
        echo "differ from custody"
    fi
}

status() {
    local photon ring_indexer ring_rpc manifest service co_signer storage fee_payer writer
    photon="$(distribution_url "$prefix-photon")"
    ring_indexer="$(distribution_url "$prefix-ring-indexer")"
    ring_rpc="$(distribution_url "$prefix-ring-rpc")"
    manifest="$(custody_manifest)"
    service="$(address "$manifest" service)"
    co_signer="$(address "$manifest" co_signer)"
    storage="$(address "$manifest" storage_authority)"
    fee_payer="$(address "$manifest" fee_payer)"
    writer="$(address "$manifest" enrollment_applier)"
    printf '%-18s %s\n' \
        "solana rpc" "$public_rpc" \
        "photon" "$photon" \
        "ring indexer" "$ring_indexer" \
        "ring rpc" "$ring_rpc" \
        "service key" "$service" \
        "co-signer key" "$co_signer" \
        "enrollment writer" "$writer"
    echo
    printf '%-18s %s   (fund it, it pays every records write)\n' "storage authority" "$storage"
    printf '%-18s %s   (fund it above the relay budget)\n' "fee payer" "$fee_payer"
    printf '%-18s %s   (fund it above the enrollment budget)\n' "enrollment writer" "$writer"
    echo
    echo "demo env, RING_PROVER_URL is the loopback prover of just ring-devnet-prover"
    printf '%s\n' "RING_SOLANA_RPC_URL=$public_rpc" "RING_INDEXER_URL=$ring_indexer" "RING_PROVER_URL=$prover_url" \
        "RING_RPC_URL=$ring_rpc" "RING_GENESIS_HASH=$genesis" "RING_RPC_SERVICE_KEY=$service" \
        "RING_RPC_COSIGNER=$co_signer" "RING_ENROLLMENT_WRITER=$writer" "RING_STORAGE_PROGRAM_ID=$storage_program"
    echo
    probe "photon" "$([[ "$photon" == - ]] && echo - || echo "$photon/liveness")"
    probe "ring indexer" "$([[ "$ring_indexer" == - ]] && echo - || echo "$ring_indexer/ready")"
    probe "ring rpc" "$([[ "$ring_rpc" == - ]] && echo - || echo "$ring_rpc/ready")"
    printf '%-18s %s\n' "ring rpc keys" "$(served_keys "$ring_rpc" "$manifest")"
    echo "logs               aws logs tail $log_group --region $region --follow"
}

# Disables every distribution first, the slow propagation then runs once for all of them.
remove_distributions() {
    local name id etag ids=() etags=()
    for name in "$@"; do
        id="$(own_distribution "$name")"
        absent "$id" && continue
        [[ "$id" != "$protected_distribution" ]] || die "refusing to touch $protected_distribution"
        etag="$(aws_ cloudfront get-distribution-config --id "$id" --query ETag --output text)"
        aws_ cloudfront get-distribution-config --id "$id" --query DistributionConfig --output json \
            | jq '.Enabled = false' > "$work/$id.json"
        etag="$(aws_ cloudfront update-distribution --id "$id" --if-match "$etag" \
            --distribution-config "file://$work/$id.json" --query ETag --output text)"
        ids+=("$id")
        etags+=("$etag")
    done
    local i
    for i in "${!ids[@]}"; do
        aws_ cloudfront wait distribution-deployed --id "${ids[$i]}"
        aws_ cloudfront delete-distribution --id "${ids[$i]}" --if-match "${etags[$i]}"
    done
}

delete_secret() {
    absent "$(secret_arn "$1")" || aws_ secretsmanager delete-secret --secret-id "$1" --force-delete-without-recovery >/dev/null
}

retire_kms_key() {
    local name="$key_alias/$1" arn state tagged
    read -r arn state <<< "$(aws_ kms describe-key --key-id "$name" \
        --query 'KeyMetadata.[Arn,KeyState]' --output text 2>/dev/null || true)"
    absent "${arn:-None}" && return 0
    tagged="$(aws_ kms list-resource-tags --key-id "$arn" --query "Tags[?TagKey=='$prefix'].TagValue | [0]" --output text)"
    if [[ "$tagged" != 1 ]]; then
        log "kept $name, its key carries no $prefix tag"
        return 0
    fi
    aws_ kms delete-alias --alias-name "$name"
    [[ "$state" == PendingDeletion ]] || aws_ kms schedule-key-deletion --key-id "$arn" --pending-window-in-days 7 >/dev/null
}

down() {
    local service services=("$prefix-photon" "$prefix-ring-indexer" "$prefix-ring-rpc")
    for service in "${services[@]}"; do
        if [[ "$(aws_ ecs describe-services --cluster "$cluster" --services "$service" --query 'services[0].status' --output text 2>/dev/null || true)" == ACTIVE ]]; then
            aws_ ecs delete-service --cluster "$cluster" --service "$service" --force >/dev/null
        fi
    done
    aws_ ecs wait services-inactive --cluster "$cluster" --services "${services[@]}" 2>/dev/null || true
    local td
    for td in $(aws_ ecs list-task-definitions --family-prefix "$prefix-" --query 'taskDefinitionArns[]' --output text); do
        absent "$td" || aws_ ecs deregister-task-definition --task-definition "$td" >/dev/null
    done
    aws_ ecs delete-cluster --cluster "$cluster" >/dev/null 2>&1 || true
    remove_distributions "$prefix-photon" "$prefix-ring-indexer" "$prefix-ring-rpc"

    local lb group
    lb="$(aws_ elbv2 describe-load-balancers --names "$load_balancer" --query 'LoadBalancers[0].LoadBalancerArn' --output text 2>/dev/null || true)"
    if ! absent "$lb"; then
        aws_ elbv2 delete-load-balancer --load-balancer-arn "$lb"
        aws_ elbv2 wait load-balancers-deleted --load-balancer-arns "$lb"
    fi
    for service in photon ring-indexer ring-rpc; do
        group="$(target_group_arn "$service")"
        absent "$group" || aws_ elbv2 delete-target-group --target-group-arn "$group"
    done

    local deleting=()
    for service in photon ring-indexer ring-rpc; do
        absent "$(db_instance "$service")" && continue
        if [[ "$service" == ring-rpc && $purge -eq 0 ]]; then
            aws_ rds delete-db-instance --db-instance-identifier "$prefix-$service" \
                --final-db-snapshot-identifier "$prefix-$service-final-$(date +%Y%m%d%H%M%S)" >/dev/null
        else
            aws_ rds delete-db-instance --db-instance-identifier "$prefix-$service" \
                --skip-final-snapshot --delete-automated-backups >/dev/null
        fi
        deleting+=("$prefix-$service")
    done
    for service in ${deleting[@]+"${deleting[@]}"}; do
        log "waiting for $service to delete"
        aws_ rds wait db-instance-deleted --db-instance-identifier "$service"
    done
    aws_ rds delete-db-subnet-group --db-subnet-group-name "$db_subnet_group" 2>/dev/null || true

    local name sg attempt
    # The database group names the task group as a source, so it goes first.
    for name in "$db_security_group" "$security_group"; do
        sg="$(aws_ ec2 describe-security-groups --filters "Name=group-name,Values=$name" --query 'SecurityGroups[0].GroupId' --output text)"
        absent "$sg" && continue
        # Task and database interfaces release the group a little after their owners go.
        for attempt in 1 2 3 4 5 6 7 8 9 10 11 12; do
            aws_ ec2 delete-security-group --group-id "$sg" 2>/dev/null && break
            ((attempt < 12)) || die "security group $sg is still in use"
            sleep 10
        done
    done
    aws_ logs delete-log-group --log-group-name "$log_group" 2>/dev/null || true
    for service in photon ring-indexer ring-rpc; do
        delete_secret "$prefix/$service/database-password"
    done
    if ((purge)); then
        for name in "${signing_roles[@]}" wrapping; do
            retire_kms_key "${name//_/-}"
        done
        # The name filter ignores case, each name is checked again.
        for name in $(aws_ secretsmanager list-secrets --filters "Key=name,Values=$prefix/" "Key=tag-key,Values=$prefix" \
            --query "SecretList[?Tags[?Key=='$prefix' && Value=='1']].Name" --output text); do
            [[ "$name" == "$prefix/"* ]] || continue
            delete_secret "$name"
        done
        for service in ring-rpc ring-indexer ring-prover photon; do
            absent "$(aws_ ecr describe-repositories --repository-names "$prefix-$service" \
                --query 'repositories[0].repositoryName' --output text 2>/dev/null || true)" \
                || aws_ ecr delete-repository --repository-name "$prefix-$service" --force >/dev/null
        done
    fi
    if ! absent "$(aws_ iam get-role --role-name "$task_role" --query Role.Arn --output text 2>/dev/null || true)"; then
        aws_ iam delete-role-policy --role-name "$task_role" --policy-name custody 2>/dev/null || true
        aws_ iam delete-role --role-name "$task_role"
    fi
    if ! absent "$(aws_ iam get-role --role-name "$role" --query Role.Arn --output text 2>/dev/null || true)"; then
        aws_ iam detach-role-policy --role-name "$role" \
            --policy-arn arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy 2>/dev/null || true
        aws_ iam delete-role-policy --role-name "$role" --policy-name secrets 2>/dev/null || true
        aws_ iam delete-role --role-name "$role"
    fi
    if ((purge)); then
        echo "removed every $prefix resource, KMS deletes its keys after 7 days"
    else
        echo "removed the $prefix stack, kept its KMS keys, its custody manifest, its images and the final Ring RPC store snapshot"
    fi
}

case "$command" in
    up) up ;;
    status) status ;;
    down) down ;;
esac
