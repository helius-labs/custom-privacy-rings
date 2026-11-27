#!/usr/bin/env python3
"""Export prepared keys, pin hashes, and regenerate dispatch. Does not publish.

The release URL is the intended artifact location. Publish the exact prepared keys
there before merging or deployment; setup is randomized and cannot be reproduced.
"""
import argparse
import concurrent.futures
import json
import pathlib
import re
import subprocess
import tempfile
from ring_key_layout import LOCK, ROOT, catalog, digest, key_file, verifier_module, verifier_relative_path

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--keys-dir', type=pathlib.Path, default=ROOT/'target/circuit-policy-keys')
    parser.add_argument('--release', required=True)
    args = parser.parse_args()
    args.keys_dir = args.keys_dir.resolve()
    prepared = json.loads((args.keys_dir/'prepared-keys.json').read_text())
    names = {key_file(k) for k in catalog()}
    if names != prepared['keys'].keys():
        raise SystemExit('Prepared keys differ from circuit catalog')
    interface = ROOT/'program-libs/interface/src/verifying_keys'
    with tempfile.TemporaryDirectory() as temporary:
        temp = pathlib.Path(temporary)
        def export(name):
            path = args.keys_dir/name
            if digest(path) != prepared['keys'][name]['sha256']:
                raise ValueError('Key changed after preparation: '+name)
            binary = temp/(name+'.vkbin')
            module = verifier_module(name)+'.rs'
            subprocess.run([str(ROOT/'target/prover-server'),'export-vk','--keys-file',str(path),'--output',str(binary)],check=True,stdout=subprocess.DEVNULL)
            subprocess.run([str(ROOT/'zolana/target/debug/xtask'),'bsb22-vk',str(binary),str(path),str(temp),module],check=True,stdout=subprocess.DEVNULL)
            subprocess.run(['rustfmt','--config-path',str(ROOT/'rustfmt.toml'),str(temp/module)],check=True)
            return module
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
            modules = list(pool.map(export,sorted(names)))
        for module in modules:
            target = interface/verifier_relative_path(module)
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes((temp/module).read_bytes())
        # Only the verifiers of the exported keys stay.
        expected = {verifier_relative_path(module) for module in modules}
        for path in interface.rglob('*_verifying_key.rs'):
            if path.relative_to(interface) not in expected:
                path.unlink()
    lock = {'release':args.release.rstrip('/'),'keys':prepared['keys']}
    LOCK.write_text(json.dumps(lock,indent=2)+'\n')
    subprocess.run(['python3',str(ROOT/'tools/generate_ring_keys.py')],check=True)
    costs_path = ROOT/'prover/ringtest/ringkey/constraints.json'
    costs = {name: count for name, count in json.loads(costs_path.read_text()).items() if name+'.key' in names}
    for name in sorted(names):
        log = args.keys_dir/(name+'.log')
        if log.exists():
            match = re.search(r'nbConstraints=(\d+)',re.sub(r'\x1b\[[0-9;]*m','',log.read_text()))
            if match: costs[name.removesuffix('.key')] = int(match[1])
    if len(costs)!=len(names): raise SystemExit('Missing constraint counts from setup logs')
    costs_path.write_text(json.dumps(dict(sorted(costs.items())),indent=2)+'\n')
    print(f'Exported and pinned {len(names)} keys. Release publication is still required.')

if __name__ == '__main__': main()
