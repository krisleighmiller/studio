#!/usr/bin/env python3
"""Actual binary round yield, post-redaction publication cap and history resume. Loopback only."""
import hashlib, http.server, json, os, pathlib, queue, subprocess, sys, tempfile, threading, uuid

# Remove only this harness's exact, uniquely named synthetic keyring entries and
# verify absence without retrieving any saved secret values.
def clear_fixture(api_id):
    cleanup = '''import gi,json,re,sys
assert re.fullmatch(r"yield-stdio-(?:[0-9a-f]{32}|local)",sys.argv[1])
gi.require_version("Secret","1")
from gi.repository import Secret
schema=Secret.Schema.new("org.freedesktop.Secret.Generic",Secret.SchemaFlags.DONT_MATCH_NAME,{"service":Secret.SchemaAttributeType.STRING,"username":Secret.SchemaAttributeType.STRING})
service=Secret.Service.get_sync(Secret.ServiceFlags.OPEN_SESSION,None)
removed=0
for field in ("apiKey","secretKey"):
 attrs={"service":"video-workbench","username":sys.argv[1]+"::"+field}
 items=service.search_sync(schema,attrs,Secret.SearchFlags.ALL,None)
 for item in items:
  assert all(item.get_attributes().get(k)==v for k,v in attrs.items())
  item.delete_sync(None);removed+=1
 assert not service.search_sync(schema,attrs,Secret.SearchFlags.ALL,None)
print(json.dumps({"fixtureApiId":sys.argv[1],"removed":removed,"remaining":0}))
'''
    result=subprocess.run(['/usr/bin/python3','-c',cleanup,api_id],capture_output=True,text=True,timeout=15)
    assert result.returncode==0,result.stderr
    print(result.stdout.strip())

def run(binary, discriminator=True):
    with tempfile.TemporaryDirectory(prefix='h03-stdio-') as tmp:
        base=pathlib.Path(tmp); root=base/'project'; (root/'Research').mkdir(parents=True)
        source=root/'Research'/'facts.txt'; source.write_text('redacted'*6000+'🦀')
        original=hashlib.sha256(source.read_bytes()).hexdigest()
        requests=[]; api_id='yield-stdio-'+uuid.uuid4().hex
        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                requests.append(json.loads(self.rfile.read(int(self.headers['Content-Length']))))
                proposal=({'action':'inspect','op':'read_file','inputs':{'path':'Research/facts.txt' if len(requests)<12 else 'Research/final-missing-unique.txt'}} if len(requests)<=12 else {'action':'done','message':'Summary complete without authorization.'})
                content='<director_action>'+json.dumps(proposal)+'</director_action>'
                body=json.dumps({'choices':[{'message':{'content':content}}]}).encode()
                self.send_response(200); self.send_header('Content-Type','application/json'); self.send_header('Content-Length',str(len(body))); self.end_headers(); self.wfile.write(body)
            def log_message(self, format, *args): pass
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler); thread=threading.Thread(target=server.serve_forever,daemon=True); thread.start()
        settings=base/'settings'; settings.mkdir()
        # Deliberately synthetic placeholder-name credential exposes non-idempotent
        # redaction. No real credentials, keyring IDs or external endpoints.
        (settings/'settings.json').write_text(json.dumps({'apis':[{'id':api_id,'name':'Loopback only','kind':'ollama','enabled':True,'baseUrl':f'http://127.0.0.1:{server.server_port}','apiKey':'redacted','secretKey':'yield:12' if discriminator else ''}],'capabilityDefaults':{}}))
        env=os.environ.copy(); env['WORKBENCH_SETTINGS_DIR']=str(settings)
        child=subprocess.Popen([binary],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=env)
        assert child.stdin is not None and child.stdout is not None and child.stderr is not None
        stdin,stdout,stderr=child.stdin,child.stdout,child.stderr
        lines=queue.Queue()
        reader=threading.Thread(target=lambda:[lines.put(json.loads(line)) for line in stdout],daemon=True); reader.start()
        def call(identity,cmd,args):
            stdin.write(json.dumps({'id':identity,'cmd':cmd,'args':args})+'\n'); stdin.flush()
            while True:
                response=lines.get(timeout=30)
                if response.get('id')==identity: return response
        try:
            assert call('open','open_project',{'path':str(root)})['ok']
            intent='Inspect facts for scene seven; retain original scope, do not generate media.'
            request={'model':'fixture','apiId':api_id,'messages':[{'role':'user','content':intent}],'context':{'projectRoot':str(root),'selectedPath':'Research','workflowSceneId':7,'workflowSceneStableId':'original-scene-seven'}}
            result=call('yield','director_turn',{'request':request})
            assert result['ok'],result
            response=result['result']; size=len(response['reply'].encode())
            assert response['continueSuggested'] and not response['stopped'],response
            assert size<=32768, f'STDIO_PUBLICATION_CAP_EXCEEDED: {size} bytes'
            assert len(requests)==12,len(requests)
            assert 'Research/facts.txt' in response['reply'],response
            assert 'Research/final-missing-unique.txt' in response['reply'] and 'read_file:' in response['reply'], 'FINAL_ROUND_ERROR_ERASED'
            assert 'pending' in response['reply'] and 'omitted' in response['reply']
            request['messages'].append({'role':'assistant','content':response['reply']})
            final=call('resume','director_turn',{'request':request})
            assert final['ok'] and not final['result']['continueSuggested'],final
            assert len(requests)==13
            messages=requests[-1]['messages']; assert messages[1]=={'role':'user','content':intent}
            assert messages[-1]=={'role':'assistant','content':response['reply']}
            assert 'original-scene-seven' in messages[0]['content']
            assert hashlib.sha256(source.read_bytes()).hexdigest()==original
            report={'verdict':'PASS','binary':str(pathlib.Path(binary).resolve()),'binarySha256':hashlib.sha256(pathlib.Path(binary).read_bytes()).hexdigest(),'yieldReplyBytes':size,'httpRequests':len(requests),'yield':response,'final':final,'originalIntent':intent,'scope':'original-scene-seven','sourceSha256':original}
            evidence=os.environ.get('H03_EVIDENCE_DIR')
            if evidence: pathlib.Path(evidence,'stdio-yield-discriminator.json' if discriminator else 'stdio-yield-expansion.json').write_text(json.dumps(report,indent=2))
            print(json.dumps({k:v for k,v in report.items() if k not in ('yield','final')}))
        finally:
            stdin.close()
            try: child.wait(timeout=10)
            except subprocess.TimeoutExpired: child.kill(); child.wait(timeout=10)
            reader.join(timeout=5); server.shutdown(); server.server_close(); thread.join(timeout=5)
            clear_fixture(api_id)
            assert child.returncode==0,stderr.read()

if __name__=='__main__': run(str(pathlib.Path(sys.argv[1]).resolve()), '--no-discriminator' not in sys.argv[2:])
