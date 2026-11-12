#!/usr/bin/env python3
"""Local MOCK-provider integration only. Never contacts Atlas or user projects.
Reuse Studio's native mock-provider API override + stdio command protocol.
Verifies standalone speech stops at a Draft; video is an optional separate approved action.
"""
from pathlib import Path
import argparse, collections, hashlib, http.server, json, os, re, selectors
import subprocess, tempfile, threading, time

class Rpc:
    def __init__(self, binary, settings, base, log):
        env=dict(os.environ,WORKBENCH_SETTINGS_DIR=str(settings),WORKBENCH_ATLAS_BASE_URL=base)
        self.stderr=log.open('wb');self.p=subprocess.Popen([binary],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=self.stderr,env=env)
        assert self.p.stdout is not None and self.p.stdin is not None
        self.buf=b'';self.events=[];self.seq=0
    def read(self, deadline):
        assert self.p.stdout is not None
        while b'\n' not in self.buf:
            left=deadline-time.monotonic()
            if left<=0:raise TimeoutError('native response deadline')
            with selectors.DefaultSelector() as s:
                s.register(self.p.stdout,selectors.EVENT_READ)
                if not s.select(left):raise TimeoutError('native response deadline')
            chunk=os.read(self.p.stdout.fileno(),65536)
            if not chunk:raise RuntimeError('native host exited')
            self.buf+=chunk
        line,self.buf=self.buf.split(b'\n',1);return json.loads(line)
    def call(self,cmd,args=None):
        assert self.p.stdin is not None
        self.seq+=1;ident=str(self.seq);self.p.stdin.write((json.dumps({'id':ident,'cmd':cmd,'args':args or {}})+'\n').encode());self.p.stdin.flush();deadline=time.monotonic()+45
        while True:
            v=self.read(deadline)
            if v.get('event'):self.events.append(v);continue
            if v.get('id')==ident:
                assert v.get('ok'),(cmd,v.get('error'));return v['result']
    def close(self):
        self.p.terminate()
        try:self.p.wait(timeout=10)
        except subprocess.TimeoutExpired:self.p.kill();self.p.wait(timeout=5)
        self.stderr.close()

def token(reply):
    m=re.search(r'\[\[workbench_approve:([A-Za-z0-9]+)\]\]',reply)
    assert m,('native approval missing',reply);return m.group(1)

def main():
    ap=argparse.ArgumentParser();ap.add_argument('--host',required=True);ap.add_argument('--evidence',required=True);ap.add_argument('--profile',action='store_true');ap.add_argument('--video',action='store_true');ap.add_argument('--pending-audio',action='store_true');ap.add_argument('--failed-audio',action='store_true');ap.add_argument('--collect-twice',action='store_true');ap.add_argument('--negative',choices=['stop-before-confirm','same-root-reopen','input-change','route-change','newer-card','duplicate-confirmation']);a=ap.parse_args()
    evidence=Path(a.evidence);evidence.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='text-sync-flow-',dir=str(evidence)) as td:
        work=Path(td);root=work/'project';settings=work/'settings';settings.mkdir();root.mkdir()
        voice=root/'Characters/Mom/voice/fixture_voice.wav';frame=root/'Preproduction/Scene1/start_frames/mom.png'
        for p in [voice,frame]:p.parent.mkdir(parents=True,exist_ok=True)
        artifact_audio=work/'delivered.wav';artifact_video=work/'delivered.mp4'
        def ff(args):subprocess.run(['ffmpeg','-nostdin','-v','error','-y']+args,check=True,capture_output=True)
        ff(['-f','lavfi','-i','sine=frequency=440:duration=1','-ar','24000','-ac','1',str(voice)])
        ff(['-f','lavfi','-i','sine=frequency=660:duration=1.25','-ar','24000','-ac','1',str(artifact_audio)])
        ff(['-f','lavfi','-i','color=c=blue:s=90x160:d=1','-frames:v','1',str(frame)])
        ff(['-f','lavfi','-i','color=c=blue:s=90x160:r=10:d=5','-f','lavfi','-i','sine=frequency=440:duration=5','-c:v','libx264','-pix_fmt','yuv420p','-c:a','aac','-shortest',str(artifact_video)])
        counts=collections.Counter();bodies={};lock=threading.Lock();audio_ready=not a.pending_audio
        class Handler(http.server.BaseHTTPRequestHandler):
            protocol_version='HTTP/1.0'
            def log_message(self,format,*args):pass
            def send(self,value,kind='application/json'):
                body=value if isinstance(value,bytes) else json.dumps(value).encode();self.send_response(200);self.send_header('Content-Type',kind);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
            def do_GET(self):
                if self.path=='/api/v1/models':
                    self.send({'data':[{'model':'bytedance/seed-audio-1.0','type':'Audio','schema':base+'/schema/audio'},{'model':'atlascloud/van-2.5/image-to-video','type':'Video','schema':base+'/schema/video'}]})
                elif self.path.startswith('/api/v1/schema/'):
                    audio=self.path.endswith('audio');props={'text':{'type':'string'},'references':{'type':'array'},'voice_id':{'type':'string'},'language':{'type':'string'}} if audio else {'prompt':{'type':'string'},'image':{'type':'string'},'audio':{'type':'string'},'duration':{'type':'integer'},'resolution':{'type':'string'},'negative_prompt':{'type':'string'},'enable_prompt_expansion':{'type':'boolean'}}
                    self.send({'type':'object','properties':props,'required':['text'] if audio else ['prompt','image','resolution']})
                elif '/model/prediction/' in self.path:
                    pred=self.path.rsplit('/',1)[-1];name='audio' if pred.startswith('audio') else 'video'
                    if name=='audio' and not audio_ready:self.send_error(404,'fixture transport interruption');return
                    self.send({'id':pred,'status':'failed' if a.failed_audio and audio_ready else 'completed','outputs':[] if a.failed_audio and audio_ready else [base+'/file/'+name]})
                elif self.path=='/api/v1/file/audio':self.send(artifact_audio.read_bytes(),'audio/wav')
                elif self.path=='/api/v1/file/video':self.send(artifact_video.read_bytes(),'video/mp4')
                else:self.send_error(404,self.path)
            def do_POST(self):
                data=self.rfile.read(int(self.headers.get('Content-Length','0')))
                if self.path=='/api/v1/model/uploadMedia':
                    if b'fixture_voice.wav' in data:kind='clone_uploads'
                    elif b'.wav' in data:kind='performance_uploads'
                    else:kind='image_uploads'
                    with lock:counts[kind]+=1;n=counts[kind]
                    self.send({'data':{'download_url':base+'/upload/'+kind+'/'+str(n)}})
                elif self.path in ['/api/v1/model/generateAudio','/api/v1/model/generateVideo']:
                    name='audio' if self.path.endswith('generateAudio') else 'video'
                    with lock:counts[name+'_posts']+=1;bodies[name]=json.loads(data)
                    self.send({'id':name+'1','status':'starting' if name=='audio' and a.pending_audio else 'completed','outputs':[] if name=='audio' and a.pending_audio else [base+'/file/'+name]})
                else:self.send_error(404,self.path)
        class Server(http.server.ThreadingHTTPServer):daemon_threads=True
        server=Server(('127.0.0.1',0),Handler);base='http://127.0.0.1:'+str(server.server_port)+'/api/v1';thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start();rpc=None
        try:
            cut={'id':{'source_scene':1,'cut':1},'stable_id':'shot_1','title':'Mom fixture','dialogue':[{'speaker':'Mom','text':'This is the exact fixture line.'}],'speech_mode':'source_video_lipsync','scene_type':'character_performance','subject':'Mom','render_lane':'audio_driven_i2v','prompt':'A synthetic fixture portrait speaks naturally.','negative_prompt':'','production_note_path':'Scene_Prompts/Scene1/Cut1.txt','approved_start_frame':str(frame.relative_to(root)),'visual_source':{'strategy':'preproduction_start_frame','asset_id':'frame'}}
            cut['sketch']={'title':'Mom fixture','shot_role':'performance','speaker':'Mom','mouth_policy':'speaking_visible','visual_prompt':cut['prompt'],'negative_prompt':'','dialogue':[{'speaker':'Mom','text':'This is the exact fixture line.','heard_as':'lip_sync'}]}
            state={'version':2,'state_revision':0,'settings':{'aspect_ratio':'9:16'},'characters':[{'id':'mom','name':'Mom','dir':'Characters/Mom','identity_ready':True,'voice_ready':True,'asset_ids':['voice']}],'scenes':[{'id':1,'title':'Fixture scene','stable_id':'scene_1','cuts':[cut]}],'assets':[{'id':'voice','path':str(voice.relative_to(root)),'role':'voice_sample','status':'present','character_id':'mom'},{'id':'frame','path':str(frame.relative_to(root)),'role':'cut_start_frame','status':'approved','scene_id':1,'cut_id':cut['id']}],'jobs':[]}
            (root/'Preproduction').mkdir(exist_ok=True);(root/'Preproduction/workflow_state.json').write_text(json.dumps(state))
            (settings/'settings.json').write_text(json.dumps({'apis':[{'id':'fixture-atlas','name':'Local synthetic fixture','kind':'atlas-cloud','baseUrl':base,'apiKey':'fixture-key','enabled':True}],'capabilityDefaults':{'audio.tts_clone':{'apiId':'fixture-atlas','modelId':'bytedance/seed-audio-1.0'},'video.lip_sync_text_narration':{'apiId':'fixture-atlas','modelId':'atlascloud/van-2.5/image-to-video'}}}))
            note=root/'Scene_Prompts/Scene1/Cut1.txt';note.parent.mkdir(parents=True,exist_ok=True)
            note.write_text('Source_Scene: 1\nCut: 1\nCharacter_Reference: '+str(frame)+'\nVoice: '+str(voice)+'\nOutput: Scenes/Scene1/Cut1\nFilenames: "S01C01.mp4" and "S01C01.wav"\n\nPrompt:\n\n```yaml\nsource_scene: 1\ncut: 1\ntitle: Mom fixture\nprovider: atlascloud\nstage: base_video\nscene_type: character_performance\ncontinuity:\n  group: fixture\n  relation_to_previous: new_setup\n  opening_state: Synthetic portrait\n  closing_state: Synthetic portrait\nediting:\n  estimated_screen_duration_seconds: 5.0\n  head_handle_seconds: 0.2\n  tail_handle_seconds: 0.2\n  transition_in: cut\n  transition_out: cut\n  continuity_frame_strategy: none\nspeech:\n  mode: source_video_lipsync\n  language: en\n  cues:\n  - id: S01C01_a\n    speaker: Mom\n    text: This is the exact fixture line.\n    on_camera: true\n    delivery: calm\n    estimated_duration_seconds: 1.0\ngeneration_requirements:\n  source_mode: reference_to_video\n  generation_path: audio_driven_i2v\n  aspect_ratio: "9:16"\n  required_capabilities: [character_reference]\nprompt: A synthetic fixture portrait speaks naturally.\nreference_assets:\n- path: '+str(frame)+'\n  role: character_reference\nnegative_prompt: ""\ncast:\n- speaker: Mom\n  character: Mom\n  reference_image: '+str(frame)+'\n```\n')
            if a.profile:
                voice=root/'Characters/Mom/voice/profile.yaml';voice.write_text('voice_id: chosen-fixture-voice\nlanguage: en\n')
                state['assets'][0]['path']=str(voice.relative_to(root));state['assets'][0]['role']='voice_profile'
            note_text=note.read_text();frame.unlink();note.unlink()
            state['scenes'][0]['cuts'][0].pop('visual_source');state['scenes'][0]['cuts'][0].pop('approved_start_frame');state['scenes'][0]['cuts'][0].pop('production_note_path');state['scenes'][0]['cuts'][0].pop('prompt');state['assets']=[state['assets'][0]]
            state['scenes'][0]['cuts'][0]['dialogue'].append({'speaker':'Mom','text':'This is also the complete second line.'})
            (root/'Preproduction/workflow_state.json').write_text(json.dumps(state))
            config=json.loads((settings/'settings.json').read_text());config['capabilityDefaults'].pop('video.lip_sync_text_narration')
            if a.profile:config['capabilityDefaults']['audio.tts_profile']=config['capabilityDefaults'].pop('audio.tts_clone')
            (settings/'settings.json').write_text(json.dumps(config))
            rpc=Rpc(a.host,settings,base,evidence/'host.stderr');assert rpc.call('host_build_info')['mock_provider'] is True,'Refusing nonmock host: no provider action attempted'
            rpc.call('open_project',{'path':str(root)})
            prepared=rpc.call('workflow_prepare_dialogue_audio',{'projectRoot':str(root),'sceneId':1,'cut':1,'voiceAssetId':'voice'})
            action=token(prepared['pendingAction'])
            first=rpc.call('director_approve',{'id':action,'progressId':'originalfixtureowner'})
            assert 'approve:awaiting-spend-authorization' in first['toolEvents'],first
            assert counts['audio_posts']==0 and counts['video_posts']==0,dict(counts)
            payload=next(e['payload'] for e in reversed(rpc.events) if e.get('event')=='spend-confirmation-required')
            assert payload['operation']=='atlas_generate_audio' and payload['workflowContinuationBound'] and payload['ownerRequestId']=='originalfixtureowner',payload
            q=payload['quote']
            def confirm():
                permit=rpc.call('confirm_unbounded_spend',{'projectRoot':str(root),'provider':payload['provider'],'category':'media','operation':'atlas_generate_audio','model':payload['model'],'requestHash':q['requestHash'],'ledgerRevision':q['ledgerRevision']})
                replay={'projectRoot':str(root),'operation':'atlas_generate_audio','mediaArgs':payload['args'],'token':permit}
                return rpc.call('media_generate_confirmed',replay),replay
            def refused(call):
                try:call()
                except AssertionError as e:return str(e)
                raise AssertionError('unsafe action was accepted')
            if a.negative and a.negative!='duplicate-confirmation':
                if a.negative=='stop-before-confirm':rpc.call('director_cancel')
                elif a.negative=='same-root-reopen':rpc.call('open_project',{'path':str(root)})
                elif a.negative=='input-change':voice.write_bytes(voice.read_bytes()+b'changed')
                elif a.negative=='route-change':
                    config['capabilityDefaults']['audio.tts_clone']['modelId']='changed/audio';(settings/'settings.json').write_text(json.dumps(config))
                elif a.negative=='newer-card':rpc.call('workflow_prepare_dialogue_audio',{'projectRoot':str(root),'sceneId':1,'cut':1,'voiceAssetId':'voice'})
                refused(confirm);assert counts['audio_posts']==0,dict(counts)
            else:
                response,replay=confirm();assert isinstance(response,dict) and response.get('continuation'),response
                if a.pending_audio:
                    collect=response['continuation'];assert 'workbench_approve:' in collect['reply'],collect
                    if a.collect_twice:
                        collect=rpc.call('director_approve',{'id':token(collect['reply']),'progressId':'firstcollectowner'})
                        assert 'workbench_approve:' in collect['reply'] and collect.get('workflowContinuation'),collect
                    audio_ready=True
                    done=rpc.call('director_approve',{'id':token(collect['reply']),'progressId':'finalcollectowner'})
                else:done=response['continuation']
                assert done.get('workflowContinuation') and 'workbench_approve:' not in done['reply'],done
                final=json.loads((root/'Preproduction/workflow_state.json').read_text())
                parent=next(j for j in final['jobs'] if j['kind']=='dialogue_audio')
                if a.failed_audio:
                    assert parent['status']=='failed',parent
                else:
                    assert parent['status']=='completed',parent
                    assets=[x for x in final['assets'] if x.get('source_job_id')==parent['id']];assert len(assets)==1,assets
                    asset=assets[0];assert asset['status']=='draft' and asset['cut_id']=={'source_scene':1,'cut':1},asset
                    assert (root/asset['path']).read_bytes()==artifact_audio.read_bytes()
                    assert not final['scenes'][0]['cuts'][0].get('selected_performance_asset_id')
                    assert bodies['audio']['text']==('' if a.profile else 'Use the voice and delivery of @audio1 and say: ')+'This is the exact fixture line. This is also the complete second line.',bodies
                    if a.profile:assert bodies['audio']['voice_id']=='chosen-fixture-voice',bodies
                assert counts['audio_posts']==1 and counts['clone_uploads']==(0 if a.profile else 1),dict(counts)
                assert all(j['status'] in ['completed','failed'] for j in final['jobs'] if j['kind']=='atlas_generate_audio'),final['jobs']
                assert not any(j['kind']=='render' for j in final['jobs'])
                if a.negative=='duplicate-confirmation':refused(lambda:rpc.call('media_generate_confirmed',replay));assert counts['audio_posts']==1
            assert counts['video_posts']==0 and counts['image_uploads']==0 and counts['performance_uploads']==0,dict(counts)
            if a.video:
                assert not a.negative and not a.failed_audio
                asset=next(x for x in final["assets"] if x.get("source_job_id")==parent["id"])
                ff(['-f','lavfi','-i','color=c=blue:s=90x160:d=1','-frames:v','1',str(frame)])
                note.write_text(note_text.replace('This is the exact fixture line.','This is the exact fixture line. This is also the complete second line.'))
                config['capabilityDefaults']['video.lip_sync_text_narration']={'apiId':'fixture-atlas','modelId':'atlascloud/van-2.5/image-to-video'}
                (settings/'settings.json').write_text(json.dumps(config))
                final=json.loads((root/'Preproduction/workflow_state.json').read_text());c=final['scenes'][0]['cuts'][0]
                c['production_note_path']=str(note.relative_to(root));c['prompt']='A synthetic fixture portrait speaks naturally.';c['approved_start_frame']=str(frame.relative_to(root));c['visual_source']={'strategy':'preproduction_start_frame','asset_id':'frame'}
                final['assets'].append({'id':'frame','path':str(frame.relative_to(root)),'role':'cut_start_frame','status':'approved','scene_id':1,'cut_id':c['id']})
                (root/'Preproduction/workflow_state.json').write_text(json.dumps(final))
                rpc.call('workflow_select_performance',{'projectRoot':str(root),'sceneId':1,'cut':1,'assetId':asset['id'],'reviewedAssetId':asset['id']})
                prepared_video=rpc.call('workflow_prepare_render',{'projectRoot':str(root),'sceneId':1,'cut':1,'selectedPerformance':True,'reviewedAssetId':asset['id']})
                assert 'TTS requests: 0' in prepared_video['pendingAction']
                video_first=rpc.call('director_approve',{'id':token(prepared_video['pendingAction']),'progressId':'videoonlyowner'})
                assert 'approve:awaiting-spend-authorization' in video_first['toolEvents'],video_first
                vp=next(e['payload'] for e in reversed(rpc.events) if e.get('event')=='spend-confirmation-required')
                assert vp['operation']=='atlas_generate_video' and vp['ownerRequestId']=='videoonlyowner',vp
                vq=vp['quote'];permit=rpc.call('confirm_unbounded_spend',{'projectRoot':str(root),'provider':vp['provider'],'category':'media','operation':vp['operation'],'model':vp['model'],'requestHash':vq['requestHash'],'ledgerRevision':vq['ledgerRevision']})
                done_video=rpc.call('media_generate_confirmed',{'projectRoot':str(root),'operation':vp['operation'],'mediaArgs':vp['args'],'token':permit})
                assert done_video.get('continuation',{}).get('workflowContinuation'),done_video
                assert counts['audio_posts']==1 and counts['video_posts']==1 and counts['clone_uploads']==(0 if a.profile else 1) and counts['image_uploads']==1 and counts['performance_uploads']==1,dict(counts)
                assert (root/asset['path']).read_bytes()==artifact_audio.read_bytes()
                final=json.loads((root/'Preproduction/workflow_state.json').read_text());assert any(j['kind']=='render' and j['status']=='completed' for j in final['jobs']),final['jobs']
            result={'proof_type':'native_loopback_mock_provider_audio_split','case':a.negative or ('failed-collection' if a.failed_audio else 'deferred-collection' if a.pending_audio else 'immediate-audio'),'counts':dict(counts),'live_provider_calls':0,'passed':True}
            (evidence/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
        finally:
            (evidence/'mock-counts.json').write_text(json.dumps(dict(counts),indent=2)+'\n')
            (evidence/'mock-bodies.json').write_text(json.dumps(bodies,indent=2)+'\n')
            state_file=root/'Preproduction/workflow_state.json'
            if state_file.exists():(evidence/'final-state.json').write_text(state_file.read_text())
            if rpc:rpc.close()
            server.shutdown();server.server_close();thread.join(timeout=5);assert not thread.is_alive()

if __name__=='__main__':main()