#!/usr/bin/env python3
"""Real stdio/eight-worker admission regression. Local settings, no credentials."""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import queue
import subprocess
import tempfile
import threading
import time


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    arrived = queue.Queue()
    responses = queue.Queue()
    release = threading.Event()
    requests = []
    lock = threading.Lock()
    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, format, *args):
            pass
        def do_POST(self):
            body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            with lock:
                requests.append(body)
            arrived.put(True)
            if not release.wait(15):
                raise TimeoutError('fixture release timed out')
            content = '<director_action>\n{"action":"done","message":"Fixture complete"}\n</director_action>'
            payload = json.dumps({'choices':[{'message':{'content':content}}]}).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    server_thread = threading.Thread(target=server.serve_forever)
    server_thread.start()
    with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR'], prefix='stdio-') as d:
        root = Path(d)
        (root/'settings.json').write_text(json.dumps({'apis':[{'id':'stdio-local','name':'Local stdio fixture','kind':'ollama','baseUrl':f'http://127.0.0.1:{server.server_port}','apiKey':'','secretKey':'','enabled':True}], 'capabilityDefaults':{}}))
        env = os.environ.copy()
        env['WORKBENCH_SETTINGS_DIR'] = str(root)
        process = subprocess.Popen([str(binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env)
        assert process.stdin is not None and process.stdout is not None
        stdin, stdout = process.stdin, process.stdout
        def reader():
            for line in stdout:
                value = json.loads(line)
                if 'id' in value:
                    responses.put(value)
        reader_thread = threading.Thread(target=reader)
        reader_thread.start()
        def send(id_, cmd, args_):
            stdin.write(json.dumps({'id':id_,'cmd':cmd,'args':args_})+'\n')
            stdin.flush()
        request = {'request':{'model':'fixture','apiId':'stdio-local','messages':[{'role':'user','content':'Run local stdio barrier.'}],'context':{}}}
        observed = []
        try:
            for i in range(8):
                send(str(i), 'director_turn', request)
            for _ in range(8):
                arrived.get(timeout=10)
            # Ninth is queued before Stop. Stop must be handled by input reader,
            # even though EVERY worker is blocked and ninth has no slot.
            queued_ids = ['queued'] + [f'queued-{i}' for i in range(63)]
            for id_ in queued_ids:
                send(id_, 'director_turn', request)
            send('overflow', 'director_turn', request)
            overflow = responses.get(timeout=1.5)
            assert overflow['id'] == 'overflow' and overflow['ok'] is False, overflow
            assert 'not executed' in overflow['error'], overflow
            observed.append(overflow)
            send('stop', 'director_cancel', {})
            try:
                stop = responses.get(timeout=1.5)
            except queue.Empty:
                raise AssertionError('Stop acknowledgement starved behind all eight worker slots')
            observed.append(stop)
            assert stop['id'] == 'stop' and stop['result']['cancelled'] is True, stop
            # A second Stop must not resurrect requests. A fresh request after
            # both generations is not poisoned by either cancellation.
            send('stop-again', 'director_cancel', {})
            repeat = responses.get(timeout=1.5)
            observed.append(repeat)
            assert repeat['id'] == 'stop-again'
            release.set()
            deadline = time.monotonic()+12
            while len(observed) < 8+len(queued_ids)+3:
                observed.append(responses.get(timeout=max(.01, deadline-time.monotonic())))
            by_id = {r['id']:r for r in observed}
            for id_ in [str(i) for i in range(8)]+queued_ids:
                result = by_id[id_]['result']
                assert result['stopped'] is True and result['continueSuggested'] is False, by_id[id_]
            assert len(requests) == 8, f'queued stale request reached HTTP: {len(requests)}'
            send('fresh', 'director_turn', request)
            fresh = responses.get(timeout=10)
            observed.append(fresh)
            assert fresh['id'] == 'fresh' and fresh['result']['stopped'] is False, fresh
            assert len(requests) == 9
            print(json.dumps({'binary':str(binary),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'results':observed,'planner_requests':len(requests),'assertions':'PASS'},indent=2))
        finally:
            release.set()
            process.terminate()  # only the process this fixture created
            process.wait(timeout=5)
            reader_thread.join(timeout=5)
            server.shutdown()
            server.server_close()
            server_thread.join(timeout=5)

if __name__ == '__main__':
    main()
