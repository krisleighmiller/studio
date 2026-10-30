#!/usr/bin/env python3
"""Real stdio progress identity: concurrent requests, loopback only, no keys."""
import hashlib
import http.server
import json
import os
from pathlib import Path
import queue
import re
import subprocess
import sys
import tempfile
import threading


def main():
    binary = Path(sys.argv[1]).resolve(strict=True)
    arrived = queue.Queue()
    output = queue.Queue()
    release = threading.Event()
    replies = queue.Queue()
    requests = []
    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, format, *args):
            pass
        def do_POST(self):
            requests.append(json.loads(self.rfile.read(int(self.headers['Content-Length']))))
            arrived.put(True)
            assert release.wait(15)
            content = replies.get_nowait() if not replies.empty() else '<director_action>\n{"action":"done","message":"Local fixture completed"}\n</director_action>'
            payload = json.dumps({'choices': [{'message': {'content': content}}]}).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    thread = threading.Thread(target=server.serve_forever)
    thread.start()
    try:
        with tempfile.TemporaryDirectory(prefix='progress-', dir=os.environ['TMPDIR']) as directory:
            root = Path(directory)
            (root/'settings.json').write_text(json.dumps({'apis': [{
                'id': 'progress-local', 'name': 'Local fixture', 'kind': 'ollama',
                'baseUrl': f'http://127.0.0.1:{server.server_port}',
                'apiKey': '', 'secretKey': '', 'enabled': True}], 'capabilityDefaults': {}}))
            env = dict(os.environ, WORKBENCH_SETTINGS_DIR=str(root))
            process = subprocess.Popen([str(binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                       stderr=subprocess.PIPE, text=True, env=env)
            assert process.stdin is not None and process.stdout is not None
            stdin, stdout = process.stdin, process.stdout
            def reader():
                for line in stdout:
                    output.put(json.loads(line))
            reader_thread = threading.Thread(target=reader)
            reader_thread.start()
            observed = []
            try:
                for identity in ['operation-A', 'operation-B']:
                    args = {'progressId': identity, 'request': {'model': 'fixture', 'apiId': 'progress-local',
                            'messages': [{'role': 'user', 'content': identity}], 'context': {}}}
                    stdin.write(json.dumps({'id': identity, 'cmd': 'director_turn', 'args': args})+'\n')
                    stdin.flush()
                for _ in range(2):
                    arrived.get(timeout=10)
                release.set()
                responses = []
                while len(responses) != 2:
                    event = output.get(timeout=10)
                    observed.append(event)
                    if 'id' in event:
                        responses.append(event)
                events = [e['payload'] for e in observed if e.get('event') == 'director-progress']
                assert events, observed
                assert all(isinstance(e, dict) and e.get('requestId') in ['operation-A', 'operation-B']
                           and isinstance(e.get('message'), str) for e in events), events
                for identity in ['operation-A', 'operation-B']:
                    messages = [e['message'] for e in events if e['requestId'] == identity]
                    assert 'Director turn started…' in messages, messages
                    assert 'Planner thinking…' in messages, messages
                assert all(r['ok'] is True and r['result']['stopped'] is False for r in responses), responses
                def call(identity, command, args):
                    stdin.write(json.dumps({'id': identity, 'cmd': command, 'args': args})+'\n')
                    stdin.flush()
                    events = []
                    while True:
                        event = output.get(timeout=10)
                        observed.append(event)
                        if event.get('id') == identity:
                            return event, events
                        if event.get('event') == 'director-progress':
                            events.append(event['payload'])

                # Invalid/missing legacy IDs do not acquire another request's metadata.
                for index, progress_id in enumerate([None, '', 'x'*129, 'é', 42, 'x'*128]):
                    reply, events = call(f'filter-{index}', 'director_turn', {
                        'progressId': progress_id, 'request': {'model': 'fixture', 'apiId': 'progress-local',
                        'messages': [{'role': 'user', 'content': 'Legacy/filter control'}], 'context': {}}})
                    assert reply['ok'] is True and events, (reply, events)
                    if progress_id == 'x'*128:
                        assert all(isinstance(e, dict) and e['requestId'] == progress_id for e in events), events
                    else:
                        assert all(isinstance(e, str) for e in events), events

                # Exercise real one-time local-save approval, not merely forwarding its IPC args.
                project = root/'project'
                project.mkdir()
                (project/'source.md').write_text('Unique local input for progress approval fixture.\n')
                target = project/'Research/progress.md'
                target.parent.mkdir()
                target.write_text('Existing output must survive before approval.\n')
                reply, _ = call('open', 'open_project', {'path': str(project)})
                assert reply['ok'] is True, reply
                replies.put('<director_action>'+json.dumps({'action': 'run_specialist', 'specialist': 'production_helper',
                    'inputs': {'question': 'Make a short source digest', 'paths': ['source.md'],
                               'output_path': 'Research/progress.md'}})+'</director_action>')
                replies.put('Unique local generated digest.\n')
                staged, staged_events = call('stage', 'director_turn', {'progressId': 'stage-progress',
                    'request': {'model': 'fixture', 'apiId': 'progress-local',
                    'messages': [{'role': 'user', 'content': 'Write the source digest, ask before saving.'}],
                    'context': {'projectRoot': str(project)}}})
                assert staged['ok'] is True, staged
                token = re.search(r'\[\[workbench_approve:([A-Za-z0-9]+)\]\]', staged['result']['reply'])
                assert token, staged
                assert target.read_text() == 'Existing output must survive before approval.\n'
                request_count = len(requests)
                approved, approval_events = call('approve', 'director_approve', {
                    'id': token[1], 'progressId': 'approve-progress'})
                assert approved['ok'] is True and 'approve:error' not in approved['result']['toolEvents'], approved
                assert target.read_text() == 'Unique local generated digest.\n'
                assert approval_events and all(e['requestId'] == 'approve-progress' for e in approval_events), approval_events
                replay, _ = call('replay', 'director_approve', {'id': token[1], 'progressId': 'fresh-id-not-authority'})
                assert replay['ok'] is False or 'approve:error' in replay['result']['toolEvents'], replay
                assert target.read_text() == 'Unique local generated digest.\n'
                assert len(requests) == request_count, 'local approval/replay must not dispatch a provider'
                print(json.dumps({'assertions': 'PASS', 'binary': str(binary),
                      'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
                      'requests': requests, 'observed': observed,
                      'approval_events': approval_events, 'source_sha256': hashlib.sha256((project/'source.md').read_bytes()).hexdigest(),
                      'output_sha256': hashlib.sha256(target.read_bytes()).hexdigest(), 'replay_rejected': True}, indent=2))
            finally:
                release.set()
                process.terminate()
                process.wait(timeout=5)
                reader_thread.join(timeout=5)
    finally:
        release.set()
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


if __name__ == '__main__':
    main()
