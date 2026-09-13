"""Render only the GLB imported into an empty scene using the master's QA setup."""
from pathlib import Path
import hashlib
import json
import bpy
from mathutils import Vector

master=Path(bpy.data.filepath);bundle=master.parent;glb=bundle/'frank.glb'
if not glb.exists():glb=Path(__file__).resolve().parents[4]/'apps/frank_desktop/assets/character_model/frank.glb'
scene=bpy.context.scene
lights=[{'name':o.name,'location':tuple(o.location),'rotation':tuple(o.rotation_euler),
         'energy':o.data.energy,'color':tuple(o.data.color),'size':o.data.size} for o in scene.objects if o.type=='LIGHT']
look=scene.view_settings.look
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(glb),disable_bone_shape=True)
scene=bpy.context.scene;world=bpy.data.worlds.new('QA_World');world.use_nodes=True;scene.world=world
world.node_tree.nodes['Background'].inputs['Color'].default_value=(.32,.32,.32,1)
world.node_tree.nodes['Background'].inputs['Strength'].default_value=.35
for item in lights:
 data=bpy.data.lights.new(item['name'],'AREA');data.energy=item['energy'];data.color=item['color'];data.size=item['size'];data.shape='DISK'
 obj=bpy.data.objects.new(item['name'],data);scene.collection.objects.link(obj);obj.location=item['location'];obj.rotation_euler=item['rotation']
data=bpy.data.cameras.new('QA_Camera');data.type='ORTHO';camera=bpy.data.objects.new('QA_Camera',data);scene.collection.objects.link(camera);scene.camera=camera
scene.render.engine='CYCLES';scene.cycles.samples=48;scene.cycles.use_denoising=True
scene.render.resolution_x=720;scene.render.resolution_y=720;scene.render.resolution_percentage=100
scene.view_settings.look=look;scene.render.image_settings.file_format='PNG';scene.render.image_settings.color_mode='RGBA'
for name,position,target,scale in [('glb_front',(0,-4,.77),(0,0,.77),1.68),('glb_three_quarter',(3,-3,1.05),(0,0,.77),1.72)]:
 camera.location=position;camera.rotation_euler=(Vector(target)-camera.location).to_track_quat('-Z','Y').to_euler();data.ortho_scale=scale
 scene.render.filepath=str(bundle/'qa'/f'{name}.png');bpy.ops.render.render(write_still=True)
(bundle/'qa/roundtrip-render.json').write_text(json.dumps({'glb_sha256':hashlib.sha256(glb.read_bytes()).hexdigest(),'import':'empty scene; bone display helper disabled','views':['glb_front.png','glb_three_quarter.png']},indent=2)+'\n')
