from pathlib import Path
import hashlib,json,shutil,subprocess,sys,re,time,os
root=Path(__file__).resolve().parents[4];bundle=Path(os.environ.get('FRANK_OUTPUT_DIR',str(root/'assets/character_model/frank/candidate-1.4')))
target=root/'apps/frank_desktop/assets/character_model/frank.glb';original=target.read_bytes();candidate=(bundle/'frank.glb').read_bytes()
log=bundle/'qa/runtime-test.log';started=time.time();result=None
try:
 target.write_bytes(candidate)
 process=subprocess.Popen(['moon','run','frank-desktop:character-smoke'],cwd=root,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
 lines=[]
 for line in process.stdout:
  lines.append(line);print(line,end='',flush=True)
 code=process.wait();output=''.join(lines);log.write_text(output)
 matches=re.findall(r'FRANK_QA_SCREENSHOT=([^\r\n]+)',output)
 screenshot=None
 if matches:
  source=Path(matches[-1].strip())
  if source.exists() and source.stat().st_mtime>=started:
   screenshot='flutter_scene_runtime.png';shutil.copy2(source,bundle/'qa'/screenshot)
 result={'glb_sha256':hashlib.sha256(candidate).hexdigest(),'command':'moon run frank-desktop:character-smoke','exit_code':code,'screenshot':screenshot,'passed':code==0 and screenshot is not None}
 (bundle/'qa/runtime-validation.json').write_text(json.dumps(result,indent=2)+'\n')
finally:
 target.write_bytes(original)
 print('Restored previous runtime asset after candidate test.',flush=True)
sys.exit(0 if result and result['passed'] else 1)
