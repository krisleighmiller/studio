#!/usr/bin/python3
"""Task-owned isolated dummy fixture only; never run against a personal session."""
import json
import os
from pathlib import Path
import subprocess
import sys

assert os.environ.get('STUDIO_KEYRING_FIXTURE') == '1', 'isolated fixture opt-in required'
DUMMY = 'studio-only-noncredential-keyring-regression-2026'

def rpc(cmd, args):
    proc = subprocess.Popen(['/runtime/target/debug/workbench-host'], stdin=subprocess.PIPE,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    try:
        proc.stdin.write(json.dumps({'id': 'fixture', 'cmd': cmd, 'args': args}) + '\n')
        proc.stdin.flush()
        result = json.loads(proc.stdout.readline())
        return result
    finally:
        proc.terminate()
        proc.communicate(timeout=10)

if sys.argv[1] == 'save':
    settings = {'apis': [{'id': 'keyring_repair_dummy', 'name': 'Isolated dummy',
                         'kind': 'openai-chat', 'baseUrl': 'http://127.0.0.1:9',
                         'apiKey': DUMMY, 'secretKey': '', 'enabled': False,
                         'catalog': []}], 'capabilityDefaults': {}, 'selectedChatModel': None}
    result = rpc('save_settings', {'settings': settings})
    print(json.dumps(result), flush=True)
    assert result['ok'], result
else:
    result = rpc('load_settings', {})
    assert result['ok'], result
    api = result['result']['apis'][0]
    assert api['hasKey'] and api['apiKey'] == '', result
    print('PASS ordinary host load: hasKey=true, renderer key blank')
    found = subprocess.run(['secret-tool', 'lookup', 'service', 'video-workbench',
                            'username', 'keyring_repair_dummy::apiKey'],
                           capture_output=True, text=True, check=True)
    expected = 'studio-ui-only-noncredential-2026' if sys.argv[1] == 'load-ui' else DUMMY
    assert found.stdout.strip() == expected
    print('PASS actual Secret Service dummy round trip (value not printed)')
    path = Path(os.environ['HOME']) / '.config/video-workbench/settings.json'
    raw = path.read_text()
    assert DUMMY not in raw and expected not in raw and json.loads(raw)['apis'][0]['apiKey'] == ''
    assert path.stat().st_mode & 0o777 == 0o600
    print('PASS settings has no plaintext key; mode 0600')
