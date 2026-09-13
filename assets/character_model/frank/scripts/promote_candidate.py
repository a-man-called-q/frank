"""Promote only a reviewed bundle whose technical, visual and runtime evidence agree.

Run from ordinary Python after reviewing the contact sheets. Does not create a
visual approval: visual-review.json must already contain a real recorded review.
"""
from pathlib import Path
import hashlib
import json
import os
import shutil
import tempfile

ROOT=Path(__file__).resolve().parents[4]
SOURCE=ROOT/'assets/character_model/frank';BUNDLE=Path(os.environ.get('FRANK_OUTPUT_DIR',str(SOURCE/'candidate-1.4')));QA=BUNDLE/'qa'
VERSION='1.4';BASELINE_VERSION='1.3'
RUNTIME=ROOT/'apps/frank_desktop/assets/character_model/frank.glb'

def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def read(name):return json.loads((QA/name).read_text())
expected={'blend':sha(BUNDLE/'frank.blend'),'glb':sha(BUNDLE/'frank.glb')}
technical=read('validation.json');geometry=read('geometry-validation.json');visual=read('visual-review.json')
render=read('render-manifest.json');pose=read('pose-validation.json');runtime=read('runtime-validation.json');roundtrip=read('roundtrip-render.json')
if technical['asset_hashes']!=expected or not technical['technical_passed']:raise RuntimeError('Technical evidence does not pass for this bundle')
if geometry['blend_sha256']!=expected['blend'] or not all(geometry[k] for k in ('dimension_target_passed','profile_target_passed','landmark_target_passed')):raise RuntimeError('Reference measurements do not pass')
if not geometry.get('head_profile_target_passed',False):raise RuntimeError('Head contours do not pass')
if not render.get('complete_render_suite',False):raise RuntimeError('Render suite is partial')
if visual['asset_hashes']!=expected or not visual['visual_passed'] or visual['unresolved']:raise RuntimeError('Visual review is incomplete')
if render['blend_sha256']!=expected['blend'] or render['glb_sha256']!=expected['glb'] or pose['blend_sha256']!=expected['blend']:raise RuntimeError('Render or pose evidence is stale')
if not all(p['finite_vertices'] for p in pose['poses'].values()):raise RuntimeError('Pose has non-finite vertices')
if runtime['glb_sha256']!=expected['glb'] or not runtime['passed'] or roundtrip['glb_sha256']!=expected['glb']:raise RuntimeError('Runtime or roundtrip evidence is stale')
# Do not overwrite edits made by another task since this revision's backup.
baseline=SOURCE/'backups'/BASELINE_VERSION
if sha(SOURCE/'frank.blend')!=sha(baseline/'frank.blend') or sha(RUNTIME)!=sha(baseline/'frank.glb'):raise RuntimeError('The master or runtime asset changed since the baseline backup')
for required in ('comparison_front.png','comparison_back.png','comparison_side.png','review_contact_sheet.png','flutter_scene_runtime.png','glb_front.png','glb_three_quarter.png'):
 if not (QA/required).is_file():raise RuntimeError('Missing evidence: '+required)
# Preserve the previous QA and texture folders, including notes beyond the baseline.
archive=SOURCE/'backups'/('pre-promotion-'+VERSION)
if archive.exists():raise RuntimeError('Promotion archive already exists; inspect before retrying')
archive.mkdir()
for name in ('qa','textures'):
 shutil.copytree(SOURCE/name,archive/name)
 shutil.copytree(BUNDLE/name,SOURCE/(name+'.pending'))

def atomic_copy(source,destination):
 with tempfile.NamedTemporaryFile(dir=destination.parent,delete=False) as f:
  temp=Path(f.name);f.write(source.read_bytes())
 try:os.replace(temp,destination)
 finally:temp.unlink(missing_ok=True)
try:
 for name in ('qa','textures'):
  os.replace(SOURCE/name,archive/(name+'-original'))
  os.replace(SOURCE/(name+'.pending'),SOURCE/name)
 atomic_copy(BUNDLE/'frank.blend',SOURCE/'frank.blend')
 atomic_copy(BUNDLE/'frank.glb',RUNTIME)
 if sha(SOURCE/'frank.blend')!=expected['blend'] or sha(RUNTIME)!=expected['glb']:raise RuntimeError('Promotion readback mismatch')
except BaseException:
 atomic_copy(baseline/'frank.blend',SOURCE/'frank.blend');atomic_copy(baseline/'frank.glb',RUNTIME)
 for name in ('qa','textures'):
  if (archive/(name+'-original')).exists():
   if (SOURCE/name).exists():shutil.rmtree(SOURCE/name)
   os.replace(archive/(name+'-original'),SOURCE/name)
 raise
(SOURCE/'qa/promotion.json').write_text(json.dumps({'asset_hashes':expected,'status':'promoted','baseline':'backups/'+BASELINE_VERSION,'runtime_path':str(RUNTIME)},indent=2)+'\n')
print('Promoted Frank '+VERSION+'; master/runtime hashes match all evidence.')
