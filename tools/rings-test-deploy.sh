#!/usr/bin/env bash
# tools/rings-test-deploy.sh {provision|up|status|down}
# provision requires RINGS_TEST_SERVICE_KEY_ARN, RINGS_TEST_WRAPPING_KEY_ARN, RINGS_TEST_GENESIS_HASH and ring-rpc on PATH.
# up requires RINGS_TEST_INDEXER_URL.
# RINGS_TEST_SOLANA_RPC_URL defaults to devnet.
# down retains the KMS keys and the custody record in Parameter Store.
set -euo pipefail

usage() {
    sed -n '2,6p' "$0" | sed 's/^# \{0,1\}//' >&2
    exit 2
}

[[ $# -ge 1 ]] || usage
command="$1"
shift

root="$(git rev-parse --show-toplevel)"
cd "$root"
region="${AWS_REGION:-eu-north-1}"
prefix="zolana-rings-kms-test"
tag_spec="Key=$prefix,Value=1"
ecs_tag_spec="key=$prefix,value=1"
account="$(aws sts get-caller-identity --query Account --output text)"
registry="$account.dkr.ecr.$region.amazonaws.com"
sha="$(git rev-parse HEAD)"
branch="$(git rev-parse --abbrev-ref HEAD | tr -c 'A-Za-z0-9._\n' '-')"
cluster="$prefix"
role="$prefix-exec"
log_group="/$prefix"
task_role="$prefix-rpc"
parameter="/$prefix/custody"
security_group="$prefix-sg"
load_balancer="$prefix"
prover_port=3001
ring_rpc_port=8785
redis_image="public.ecr.aws/docker/library/redis:7.4.4-alpine3.21"
fetch_image="public.ecr.aws/docker/library/alpine:3.21"
# The prover embeds this lockfile, so reading the prefix here keeps the two aligned.
published_keys="https://d3gbdb0egjwcw9.cloudfront.net/$(python3 -c "import json;print(json.load(open('$(dirname "$0")/../zolana/prover/server/prover/provingkeys/proving-keys.lock'))['prefix'])")"

aws_() { aws --region "$region" "$@"; }

log() { printf '%s\n' "$*" >&2; }

ensure_repository() {
    aws_ ecr describe-repositories --repository-names "$1" >/dev/null 2>&1 \
        || aws_ ecr create-repository --repository-name "$1" --image-tag-mutability IMMUTABLE --tags "$tag_spec" >/dev/null
}

vpc_and_subnets() {
    local vpc="${RINGS_TEST_VPC:-}"
    if [[ -z "$vpc" ]]; then
        vpc="$(aws_ ec2 describe-vpcs --filters Name=is-default,Values=true --query 'Vpcs[0].VpcId' --output text)"
    fi
    [[ "$vpc" != None && -n "$vpc" ]] || { log "no default VPC, set RINGS_TEST_VPC"; exit 1; }
    subnets="$(aws_ ec2 describe-subnets --filters "Name=vpc-id,Values=$vpc" "Name=map-public-ip-on-launch,Values=true" \
        --query 'Subnets[].SubnetId' --output text | tr '\t' ',')"
    [[ -n "$subnets" ]] || { log "VPC $vpc has no public subnets"; exit 1; }
    echo "$vpc" "$subnets"
}

ensure_security_group() {
    local vpc="$1" id
    id="$(aws_ ec2 describe-security-groups --filters "Name=group-name,Values=$security_group" "Name=vpc-id,Values=$vpc" \
        --query 'SecurityGroups[0].GroupId' --output text)"
    if [[ "$id" == None || -z "$id" ]]; then
        id="$(aws_ ec2 create-security-group --group-name "$security_group" --description "$prefix" --vpc-id "$vpc" \
            --tag-specifications "ResourceType=security-group,Tags=[{$tag_spec}]" --query GroupId --output text)"
        for port in $prover_port $ring_rpc_port; do
            aws_ ec2 authorize-security-group-ingress --group-id "$id" --protocol tcp --port "$port" --cidr 0.0.0.0/0 >/dev/null
        done
    fi
    echo "$id"
}

ensure_role() {
    local name="$1"
    if ! aws_ iam get-role --role-name "$name" >/dev/null 2>&1; then
        aws_ iam create-role --role-name "$name" --tags "$tag_spec" --assume-role-policy-document "$(jq -n \
            --arg account "$account" --arg source "arn:aws:ecs:$region:$account:*" '{
            Version:"2012-10-17", Statement:[{Effect:"Allow", Principal:{Service:"ecs-tasks.amazonaws.com"},
                Action:"sts:AssumeRole", Condition:{StringEquals:{"aws:SourceAccount":$account},
                                                   ArnLike:{"aws:SourceArn":$source}}}]}')" >/dev/null
    fi
    aws_ iam get-role --role-name "$name" --query Role.Arn --output text
}

set_custody_policy() {
    local record="$1"
    aws_ iam put-role-policy --role-name "$task_role" --policy-name custody --policy-document \
        "$(jq -c --arg deployment "$prefix" '{
            Version:"2012-10-17", Statement:[
                {Effect:"Allow", Action:"kms:GetPublicKey", Resource:.custody.service.key_arn},
                {Effect:"Allow", Action:"kms:Sign", Resource:.custody.service.key_arn,
                 Condition:{StringEquals:{"kms:SigningAlgorithm":"ED25519_SHA_512"}}},
                {Effect:"Allow", Action:"kms:Decrypt", Resource:.custody.auditor.key_arn,
                 Condition:{StringEquals:{"kms:EncryptionContext:application":"zolana-custom-rings-v1",
                     "kms:EncryptionContext:deployment":$deployment, "kms:EncryptionContext:genesis":.genesis,
                     "kms:EncryptionContext:purpose":"auditor-root"}}}]}' <<< "$record")"
}

# Runs in the fetch container before the prover starts.
key_fetch_script() {
    local lock="zolana/prover/server/prover/provingkeys/proving-keys.lock"
    local name sha
    printf 'set -eu\ncd /keys\n'
    # shellcheck disable=SC2016
    printf 'fetch() { [ -f "$2" ] && echo "$3  $2" | sha256sum -c -s && return; wget -q -O "$2.part" "$1" && echo "$3  $2.part" | sha256sum -c -s && mv "$2.part" "$2"; }\n'
    for name in transfer_ring_1_2.key transfer_ring_2_2.key; do
        sha="$(jq -r --arg n "$name" '.keys[$n].sha256' "$lock")"
        printf 'fetch %s/%s %s %s\n' "$published_keys" "$name" "$name" "$sha"
    done
}

ensure_load_balancer() {
    local vpc="$1" subnets="$2" arn
    arn="$(aws_ elbv2 describe-load-balancers --names "$load_balancer" --query 'LoadBalancers[0].LoadBalancerArn' --output text 2>/dev/null || true)"
    if [[ "$arn" == None || -z "$arn" ]]; then
        local subnet_list
        IFS=, read -r -a subnet_list <<< "$subnets"
        arn="$(aws_ elbv2 create-load-balancer --name "$load_balancer" --type network --scheme internet-facing \
            --subnets "${subnet_list[@]}" --tags "$tag_spec" --query 'LoadBalancers[0].LoadBalancerArn' --output text)"
    fi
    # The prover task and the ring RPC task each run in one zone, and not the
    # same one. Without cross-zone the nodes of the other zones have no target,
    # accept the connection and never answer, so two of the three balancer
    # addresses time out.
    aws_ elbv2 modify-load-balancer-attributes --load-balancer-arn "$arn" \
        --attributes Key=load_balancing.cross_zone.enabled,Value=true >/dev/null
    local service port group
    for service in prover ring-rpc; do
        port="$prover_port"; [[ "$service" == prover ]] || port="$ring_rpc_port"
        group="$(aws_ elbv2 describe-target-groups --names "$prefix-$service" --query 'TargetGroups[0].TargetGroupArn' --output text 2>/dev/null || true)"
        if [[ "$group" == None || -z "$group" ]]; then
            group="$(aws_ elbv2 create-target-group --name "$prefix-$service" --protocol TCP --port "$port" --vpc-id "$vpc" \
                --target-type ip --health-check-protocol HTTP --health-check-path /health --tags "$tag_spec" \
                --query 'TargetGroups[0].TargetGroupArn' --output text)"
        fi
        if [[ -z "$(aws_ elbv2 describe-listeners --load-balancer-arn "$arn" --query "Listeners[?Port==\`$port\`].ListenerArn" --output text)" ]]; then
            aws_ elbv2 create-listener --load-balancer-arn "$arn" --protocol TCP --port "$port" \
                --default-actions "Type=forward,TargetGroupArn=$group" --tags "$tag_spec" >/dev/null
        fi
    done
    aws_ elbv2 describe-load-balancers --load-balancer-arns "$arn" --query 'LoadBalancers[0].DNSName' --output text
}

# CloudFront in front of the load balancer gives HTTPS on an amazon hostname
# with Amazon's certificate, no domain to validate.
ensure_distribution() {
    local name="$1" origin="$2" origin_port="$3" id
    id="$(aws_ cloudfront list-distributions --query "DistributionList.Items[?Comment=='$name'].Id | [0]" --output text 2>/dev/null || true)"
    if [[ "$id" == None || -z "$id" ]]; then
        id="$(aws_ cloudfront create-distribution --distribution-config "$(jq -n --arg name "$name" --arg origin "$origin" --argjson port "$origin_port" '{
            CallerReference: $name, Comment: $name, Enabled: true, HttpVersion: "http2", PriceClass: "PriceClass_100",
            Origins: {Quantity: 1, Items: [{Id: "origin", DomainName: $origin,
                CustomOriginConfig: {HTTPPort: $port, HTTPSPort: 443, OriginProtocolPolicy: "http-only",
                    OriginReadTimeout: 60, OriginKeepaliveTimeout: 5, OriginSslProtocols: {Quantity: 1, Items: ["TLSv1.2"]}}}]},
            DefaultCacheBehavior: {TargetOriginId: "origin", ViewerProtocolPolicy: "redirect-to-https",
                AllowedMethods: {Quantity: 7, Items: ["GET","HEAD","OPTIONS","PUT","POST","PATCH","DELETE"],
                    CachedMethods: {Quantity: 2, Items: ["GET","HEAD"]}},
                CachePolicyId: "4135ea2d-6df8-44a3-9df3-4b5a84be39ad",
                OriginRequestPolicyId: "b689b0a8-53d0-40ab-baf2-68738e2966ac", Compress: true}
        }')" --query 'Distribution.Id' --output text)"
        aws_ cloudfront tag-resource --resource "arn:aws:cloudfront::$account:distribution/$id" --tags "Items=[{$tag_spec}]"
    fi
    aws_ cloudfront get-distribution --id "$id" --query 'Distribution.DomainName' --output text
}

# The photon named by RINGS_TEST_INDEXER_URL belongs to another stack and is
# never changed here. It only becomes the origin of a distribution of this
# stack, so a page served over HTTPS reads it without a mixed content block.
ensure_indexer_distribution() {
    local url="$1" hostport host port=80
    [[ "$url" == http://* ]] || return 0
    hostport="${url#http://}"
    hostport="${hostport%%/*}"
    host="${hostport%%:*}"
    [[ "$hostport" != *:* ]] || port="${hostport##*:}"
    ensure_distribution "$prefix-indexer" "$host" "$port" >/dev/null
}

remove_distribution() {
    local name="$1" id etag
    id="$(aws_ cloudfront list-distributions --query "DistributionList.Items[?Comment=='$name'].Id | [0]" --output text 2>/dev/null || true)"
    [[ "$id" != None && -n "$id" ]] || return 0
    etag="$(aws_ cloudfront get-distribution-config --id "$id" --query ETag --output text)"
    aws_ cloudfront get-distribution-config --id "$id" --query DistributionConfig | jq '.Enabled = false' > "/tmp/$name.json"
    etag="$(aws_ cloudfront update-distribution --id "$id" --if-match "$etag" --distribution-config "file:///tmp/$name.json" --query ETag --output text)"
    aws_ cloudfront wait distribution-deployed --id "$id"
    aws_ cloudfront delete-distribution --id "$id" --if-match "$etag"
}

target_group_arn() {
    aws_ elbv2 describe-target-groups --names "$prefix-$1" --query 'TargetGroups[0].TargetGroupArn' --output text
}

validate_record() {
    jq -ce --arg arn "arn:aws:kms:$region:$account:key/" '
            def key_arn: startswith($arn) and (ltrimstr($arn) | test("\\A([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}|mrk-[0-9a-f]{32})\\z"));
            def address: test("\\A[1-9A-HJ-NP-Za-km-z]{32,44}\\z");
            select((keys == ["custody","genesis"]) and (.genesis | address)
                and (.custody | keys == ["auditor","service"])
                and (.custody.service | keys == ["address","key_arn"])
                and (.custody.auditor | keys == ["ciphertext","key_arn"])
                and (.custody.service.key_arn | key_arn) and (.custody.auditor.key_arn | key_arn)
                and (.custody.service.address | address)
                and (.custody.auditor.ciphertext | test("\\A[A-Za-z0-9+/]+={0,2}\\z")))'
}

custody_record() {
    aws_ ssm get-parameter --name "$parameter:1" --query Parameter.Value --output text | validate_record
}

provision() {
    [[ $# -eq 0 ]] || usage
    [[ -z "${RING_RPC_LOCALNET_KMS:-}" ]] || { log "unset RING_RPC_LOCALNET_KMS for AWS provisioning"; return 1; }
    local genesis="${RINGS_TEST_GENESIS_HASH:?set RINGS_TEST_GENESIS_HASH}"
    local signer="${RINGS_TEST_SERVICE_KEY_ARN:?set RINGS_TEST_SERVICE_KEY_ARN}"
    local wrapper="${RINGS_TEST_WRAPPING_KEY_ARN:?set RINGS_TEST_WRAPPING_KEY_ARN}"
    local existing service auditor record
    existing="$(aws_ ssm get-parameters --names "$parameter:1" --query 'length(Parameters)' --output text)"
    if [[ "$existing" == 1 ]]; then
        record="$(custody_record)"
        jq -e --arg genesis "$genesis" --arg signer "$signer" --arg wrapper "$wrapper" \
            '.genesis == $genesis and .custody.service.key_arn == $signer and .custody.auditor.key_arn == $wrapper' \
            <<< "$record" >/dev/null || { log "existing custody record differs from requested identities"; return 1; }
        return
    fi
    service="$(ring-rpc signer-ref --kms-region "$region" --key-arn "$signer")"
    auditor="$(ring-rpc provision --kms-region "$region" --deployment "$prefix" --genesis-hash "$genesis" \
        --wrapping-key-arn "$wrapper" --purpose auditor-root)"
    record="$(jq -cn --arg genesis "$genesis" --argjson service "$service" --argjson auditor "$auditor" \
        '{genesis:$genesis,custody:{service:$service,auditor:$auditor}}' | validate_record)"
    aws_ ssm put-parameter --name "$parameter" --type String --no-overwrite --tags "$tag_spec" --value "$record" >/dev/null
}

register_prover() {
    local image="$1" role_arn="$2"
    aws_ ecs register-task-definition --family "$prefix-prover" --tags "$ecs_tag_spec" \
        --requires-compatibilities FARGATE --network-mode awsvpc --cpu 4096 --memory 16384 \
        --execution-role-arn "$role_arn" --runtime-platform cpuArchitecture=X86_64,operatingSystemFamily=LINUX \
        --volumes '[{"name": "keys"}]' \
        --container-definitions "$(jq -n --arg image "$image" --arg redis "$redis_image" --arg fetch "$fetch_image" \
            --arg script "$(key_fetch_script)" --arg group "$log_group" --arg region "$region" --argjson port "$prover_port" '
            def logs(p): {logDriver: "awslogs", options: {"awslogs-group": $group, "awslogs-region": $region, "awslogs-stream-prefix": p}};
            def keys: [{sourceVolume: "keys", containerPath: "/keys"}];
            [
            {name: "redis", image: $redis, essential: true,
             command: ["redis-server", "--bind", "127.0.0.1", "--save", "", "--appendonly", "no",
                       "--crash-log-enabled", "no", "--hide-user-data-from-log", "yes"],
             logConfiguration: logs("redis")},
            {name: "fetch", image: $fetch, essential: false, user: "0", mountPoints: keys,
             command: ["sh", "-c", ($script + "\nchown -R 65532:65532 /keys")], logConfiguration: logs("fetch")},
            {name: "prover", image: $image, essential: true, mountPoints: keys,
             dependsOn: [{containerName: "redis", condition: "START"}, {containerName: "fetch", condition: "SUCCESS"}],
             command: ["start", "--keys-dir", "/keys/", "--prover-address", ("0.0.0.0:" + ($port|tostring)),
                       "--auto-download=true", "--redis-url", "redis://127.0.0.1:6379/0"],
             portMappings: [{containerPort: $port, protocol: "tcp"}],
             environment: [{name:"GOTRACEBACK",value:"none"}], logConfiguration: logs("prover")}
        ] | map(. + {ulimits:[{name:"core",hardLimit:0,softLimit:0}]})')" --query 'taskDefinition.taskDefinitionArn' --output text
}

register_ring_rpc() {
    local image="$1" role_arn="$2" task_arn="$3" record="$4" indexer="$5" rpc="$6"
    aws_ ecs register-task-definition --family "$prefix-ring-rpc" --tags "$ecs_tag_spec" \
        --requires-compatibilities FARGATE --network-mode awsvpc --cpu 512 --memory 1024 \
        --execution-role-arn "$role_arn" --task-role-arn "$task_arn" --runtime-platform cpuArchitecture=X86_64,operatingSystemFamily=LINUX \
        --container-definitions "$(jq -n --arg image "$image" --argjson record "$record" --arg deployment "$prefix" \
            --arg indexer "$indexer" --arg rpc "$rpc" --arg origins "${RINGS_TEST_ORIGINS:-*}" \
            --arg rp "${RINGS_TEST_RP_ID:-}" --arg group "$log_group" --arg region "$region" --argjson port "$ring_rpc_port" '[
            {name: "ring-rpc", image: $image, essential: true,
             readonlyRootFilesystem: true, ulimits:[{name:"core",hardLimit:0,softLimit:0}],
             environment: ([
               {name: "RING_RPC_BIND", value: "0.0.0.0"}, {name: "RING_RPC_INSECURE_PUBLIC_BIND", value: "true"},
               {name: "RING_RPC_PORT", value: ($port|tostring)},
               {name: "RING_RPC_INDEXER_URL", value: $indexer}, {name: "RING_RPC_SOLANA_RPC_URL", value: $rpc},
               {name: "RING_RPC_CUSTODY_JSON", value: ($record.custody|tojson)},
               {name: "RING_RPC_DEPLOYMENT", value: $deployment}, {name: "AWS_REGION", value: $region},
               {name: "RING_RPC_ALLOW_ORIGINS", value: $origins}]
               + (if $rp == "" then [] else [{name: "RING_RPC_WEBAUTHN_RP_ID", value: $rp}] end)),
             portMappings: [{containerPort: $port, protocol: "tcp"}],
             logConfiguration: {logDriver: "awslogs", options: {"awslogs-group": $group, "awslogs-region": $region, "awslogs-stream-prefix": "ring-rpc"}}}
        ]')" --query 'taskDefinition.taskDefinitionArn' --output text
}

ensure_service() {
    local name="$1" task_definition="$2" subnets="$3" sg="$4" container="$5" port="$6"
    local status
    status="$(aws_ ecs describe-services --cluster "$cluster" --services "$name" --query 'services[0].status' --output text 2>/dev/null || true)"
    if [[ "$status" == ACTIVE ]]; then
        aws_ ecs update-service --cluster "$cluster" --service "$name" --task-definition "$task_definition" --force-new-deployment >/dev/null
    else
        aws_ ecs create-service --cluster "$cluster" --service-name "$name" --task-definition "$task_definition" \
            --desired-count 1 --launch-type FARGATE --tags "$ecs_tag_spec" --health-check-grace-period-seconds 120 \
            --load-balancers "targetGroupArn=$(target_group_arn "$container"),containerName=$container,containerPort=$port" \
            --network-configuration "awsvpcConfiguration={subnets=[$subnets],securityGroups=[$sg],assignPublicIp=ENABLED}" >/dev/null
    fi
}

public_ip() {
    local task
    task="$(aws_ ecs list-tasks --cluster "$cluster" --service-name "$1" --desired-status RUNNING --query 'taskArns[0]' --output text 2>/dev/null || true)"
    [[ "$task" != None && -n "$task" ]] || { echo "-"; return; }
    local eni
    eni="$(aws_ ecs describe-tasks --cluster "$cluster" --tasks "$task" \
        --query "tasks[0].attachments[0].details[?name=='networkInterfaceId'].value" --output text)"
    aws_ ec2 describe-network-interfaces --network-interface-ids "$eni" --query 'NetworkInterfaces[0].Association.PublicIp' --output text
}

up() {
    [[ $# -eq 0 ]] || usage
    local indexer="${RINGS_TEST_INDEXER_URL:?set RINGS_TEST_INDEXER_URL to the photon the ring RPC reads}"
    local rpc="${RINGS_TEST_SOLANA_RPC_URL:-https://api.devnet.solana.com}"
    local prover_tag="${RINGS_TEST_PROVER_TAG:-ring-prover-$branch-${sha:0:12}}"
    local ring_rpc_tag="${RINGS_TEST_RING_RPC_TAG:-ring-rpc-$branch-${sha:0:12}}"

    local record
    record="$(custody_record)"

    log "== images"
    ensure_repository zolana-ring-prover
    ensure_repository zolana-ring-rpc
    local service tag repository
    for service in ring-prover ring-rpc; do
        tag="$prover_tag"; repository=zolana-ring-prover
        [[ "$service" == ring-prover ]] || { tag="$ring_rpc_tag"; repository=zolana-ring-rpc; }
        if ! aws_ ecr describe-images --repository-name "$repository" --image-ids "imageTag=$tag" >/dev/null 2>&1; then
            zolana/tools/publish-image.sh "$service" --repository "$repository" --tag "$tag" --push >&2
        fi
    done

    log "== network, load balancer, role, logs, cluster"
    read -r vpc subnets <<< "$(vpc_and_subnets)"
    local sg role_arn task_arn
    sg="$(ensure_security_group "$vpc")"
    local dns
    dns="$(ensure_load_balancer "$vpc" "$subnets")"
    role_arn="$(ensure_role "$role")"
    task_arn="$(ensure_role "$task_role")"
    aws_ iam attach-role-policy --role-name "$role" \
        --policy-arn arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy
    set_custody_policy "$record"
    aws_ logs describe-log-groups --log-group-name-prefix "$log_group" --query "logGroups[?logGroupName=='$log_group']" --output text | grep -q . \
        || aws_ logs create-log-group --log-group-name "$log_group" --tags "$prefix=1"
    aws_ ecs describe-clusters --clusters "$cluster" --query "clusters[?status=='ACTIVE']" --output text | grep -q . \
        || aws_ ecs create-cluster --cluster-name "$cluster" --tags "$ecs_tag_spec" >/dev/null

    log "== services"
    local prover_td ring_rpc_td
    prover_td="$(register_prover "$registry/zolana-ring-prover:$prover_tag" "$role_arn")"
    ring_rpc_td="$(register_ring_rpc "$registry/zolana-ring-rpc:$ring_rpc_tag" "$role_arn" "$task_arn" "$record" "$indexer" "$rpc")"
    ensure_service "$prefix-prover" "$prover_td" "$subnets" "$sg" prover "$prover_port"
    ensure_service "$prefix-ring-rpc" "$ring_rpc_td" "$subnets" "$sg" ring-rpc "$ring_rpc_port"
    log "== https"
    ensure_distribution "$prefix-prover" "$dns" "$prover_port" >/dev/null
    ensure_distribution "$prefix-ring-rpc" "$dns" "$ring_rpc_port" >/dev/null
    ensure_indexer_distribution "$indexer"
    log "waiting for the services to stabilize"
    aws_ ecs wait services-stable --cluster "$cluster" --services "$prefix-prover" "$prefix-ring-rpc"
    status
}

status() {
    local dns prover_host ring_rpc_host indexer_host
    dns="$(aws_ elbv2 describe-load-balancers --names "$load_balancer" --query 'LoadBalancers[0].DNSName' --output text 2>/dev/null || echo "-")"
    prover_host="$(aws_ cloudfront list-distributions --query "DistributionList.Items[?Comment=='$prefix-prover'].DomainName | [0]" --output text)"
    ring_rpc_host="$(aws_ cloudfront list-distributions --query "DistributionList.Items[?Comment=='$prefix-ring-rpc'].DomainName | [0]" --output text)"
    indexer_host="$(aws_ cloudfront list-distributions --query "DistributionList.Items[?Comment=='$prefix-indexer'].DomainName | [0]" --output text)"
    echo "prover    https://$prover_host   (http://$dns:$prover_port, task $(public_ip "$prefix-prover"))"
    echo "ring rpc  https://$ring_rpc_host   (http://$dns:$ring_rpc_port, task $(public_ip "$prefix-ring-rpc"))"
    curl -sf --max-time 15 "https://$prover_host/health" && echo || echo "prover not reachable over https yet"
    [[ "$indexer_host" == None || -z "$indexer_host" ]] || echo "indexer   https://$indexer_host   (the photon of another stack, read only)"
    curl -sf --max-time 15 "https://$ring_rpc_host/health" && echo || echo "ring rpc not reachable over https yet"
    echo "logs      aws logs tail $log_group --region $region --follow"
}

down() {
    local name
    for name in "$prefix-prover" "$prefix-ring-rpc"; do
        if [[ "$(aws_ ecs describe-services --cluster "$cluster" --services "$name" --query 'services[0].status' --output text 2>/dev/null)" == ACTIVE ]]; then
            aws_ ecs delete-service --cluster "$cluster" --service "$name" --force >/dev/null
        fi
    done
    aws_ ecs wait services-inactive --cluster "$cluster" --services "$prefix-prover" "$prefix-ring-rpc" 2>/dev/null || true
    local td
    for td in $(aws_ ecs list-task-definitions --family-prefix "$prefix-" --query 'taskDefinitionArns[]' --output text); do
        aws_ ecs deregister-task-definition --task-definition "$td" >/dev/null
    done
    aws_ ecs delete-cluster --cluster "$cluster" >/dev/null 2>&1 || true
    remove_distribution "$prefix-prover"
    remove_distribution "$prefix-ring-rpc"
    remove_distribution "$prefix-indexer"
    local lb
    lb="$(aws_ elbv2 describe-load-balancers --names "$load_balancer" --query 'LoadBalancers[0].LoadBalancerArn' --output text 2>/dev/null || true)"
    if [[ "$lb" != None && -n "$lb" ]]; then
        aws_ elbv2 delete-load-balancer --load-balancer-arn "$lb"
        aws_ elbv2 wait load-balancers-deleted --load-balancer-arns "$lb"
    fi
    local group
    for group in $(aws_ elbv2 describe-target-groups --names "$prefix-prover" "$prefix-ring-rpc" --query 'TargetGroups[].TargetGroupArn' --output text 2>/dev/null); do
        aws_ elbv2 delete-target-group --target-group-arn "$group"
    done
    local sg
    sg="$(aws_ ec2 describe-security-groups --filters "Name=group-name,Values=$security_group" --query 'SecurityGroups[0].GroupId' --output text)"
    [[ "$sg" == None || -z "$sg" ]] || { sleep 20; aws_ ec2 delete-security-group --group-id "$sg"; }
    aws_ logs delete-log-group --log-group-name "$log_group" 2>/dev/null || true
    if aws_ iam get-role --role-name "$task_role" >/dev/null 2>&1; then
        aws_ iam delete-role-policy --role-name "$task_role" --policy-name custody
        aws_ iam delete-role --role-name "$task_role"
    fi
    if aws_ iam get-role --role-name "$role" >/dev/null 2>&1; then
        aws_ iam detach-role-policy --role-name "$role" --policy-arn arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy
        aws_ iam delete-role --role-name "$role"
    fi
    echo "removed $prefix runtime resources"
}

case "$command" in
    provision) provision "$@" ;;
    up) up "$@" ;;
    status) status ;;
    down) down ;;
    *) usage ;;
esac
