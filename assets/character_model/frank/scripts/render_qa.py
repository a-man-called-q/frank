"""Render the saved master without rebuilding it or changing the saved asset."""
import bpy
import math
import hashlib
import json
import os
from pathlib import Path
from mathutils import Vector

master=Path(bpy.data.filepath);qa=master.parent/'qa';qa.mkdir(exist_ok=True)
glb=master.parent/'frank.glb'
if not glb.exists():glb=Path(__file__).resolve().parents[4]/'apps/frank_desktop/assets/character_model/frank.glb'
scene=bpy.context.scene;camera=scene.camera;rig=bpy.data.objects['Frank_Rig']
scene.render.resolution_x=720;scene.render.resolution_y=720
scene.render.engine='CYCLES';scene.cycles.samples=48;scene.cycles.use_denoising=True
views={
 'view_front':((0,-4,.77),(0,0,.77),1.68),
 'view_back':((0,4,.77),(0,0,.77),1.68),
 'view_side':((4,0,.77),(0,0,.77),1.68),
 'view_side_left':((-4,0,.77),(0,0,.77),1.68),
 'view_three_quarter_left':((-3,-3,1.05),(0,0,.77),1.72),
 'view_three_quarter_right':((3,-3,1.05),(0,0,.77),1.72),
 'closeup_face':((0,-4,1.20),(0,0,1.19),.76),
 'closeup_ear':((2,-3,1.085),(.30,0,1.085),.25),
 'closeup_foot':((.7,-3,.3),(.137,-.04,.065),.30),
 'closeup_pelvis':((0,-3,.45),(0,0,.45),.48),
 'closeup_pelvis_back':((0,3,.45),(0,0,.45),.48),
 'closeup_hand':((3,0,.40),(.375,0,.405),.27),
 'closeup_hand_palm':((-2,-1,.42),(.375,0,.405),.27),
 'closeup_neck':((0,-3,.86),(0,0,.86),.48),
 'view_reference_side':((4,-.55,.77),(0,0,.77),1.68),
 'closeup_torso_side':((4,0,.65),(0,0,.65),.65),
 'closeup_neck_back':((0,3,.86),(0,0,.86),.48),
}

def render(name,spec):
 selected=os.environ.get('FRANK_QA_VIEWS','').split(',')
 if selected!=[''] and name not in selected:return
 position,target,scale=spec;camera.location=position
 camera.rotation_euler=(Vector(target)-camera.location).to_track_quat('-Z','Y').to_euler();camera.data.ortho_scale=scale
 scene.render.filepath=str(qa/(name+'.png'));bpy.ops.render.render(write_still=True)

for name,spec in views.items():render(name,spec)
# Clay verifies the surface independently of color/UV boundaries.
clay=bpy.data.materials.new('QA_Clay');clay.diffuse_color=(.48,.48,.48,1);clay.use_nodes=True
clay.node_tree.nodes['Principled BSDF'].inputs['Base Color'].default_value=(.48,.48,.48,1)
clay.node_tree.nodes['Principled BSDF'].inputs['Roughness'].default_value=.8
scene.view_layers[0].material_override=clay
for name in ('view_front','view_back','view_side','closeup_hand','closeup_pelvis','closeup_neck','closeup_face','closeup_ear','closeup_torso_side'):render('clay_'+name,views[name])
black=bpy.data.materials.new('QA_Silhouette');black.use_nodes=True
nodes=black.node_tree.nodes;nodes.clear();out=nodes.new('ShaderNodeOutputMaterial');emission=nodes.new('ShaderNodeEmission');emission.inputs['Color'].default_value=(0,0,0,1);black.node_tree.links.new(emission.outputs[0],out.inputs[0])
scene.view_layers[0].material_override=black
for name in ('view_front','view_back','view_side'):render('silhouette_'+name,views[name])
scene.view_layers[0].material_override=None
# Execute only the inspection module's definitions, preserving its restore contract.
script=Path(__file__).resolve().with_name('inspect_poses.py')
namespace={};exec(compile(script.read_text(),str(script),'exec'),namespace)
for name,rotations in namespace['POSES'].items():
 for b in rig.pose.bones:b.matrix_basis.identity();b.rotation_mode='XYZ'
 for key,rotation in rotations.items():rig.pose.bones[key].rotation_euler=tuple(math.radians(v) for v in rotation)
 bpy.context.view_layer.update()
 render('pose_'+name,((3,-3,1.0),(0,0,.72),1.72))
 if name=='grip':render('pose_grip_close',((3,-.4,.55),(.375,0,.405),.32))
for b in rig.pose.bones:b.matrix_basis.identity()
bpy.context.view_layer.update()
(qa/'render-manifest.json').write_text(json.dumps({'blend_sha256':hashlib.sha256(master.read_bytes()).hexdigest(),
 'glb_sha256':hashlib.sha256(glb.read_bytes()).hexdigest(),
 'complete_render_suite':not bool(os.environ.get('FRANK_QA_VIEWS')),
 'engine':'Cycles, 48 samples, denoised, neutral area lighting','views':views},indent=2)+'\n')
