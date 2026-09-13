"""Validate a saved candidate in a disposable Blender process; never save the scene.

FRANK_OUTPUT_DIR selects the asset bundle. This report is technical evidence only.
The separate visual-review.json must reference the same asset hashes.
"""
from pathlib import Path
import hashlib
import json
import math
import os
import struct
import bpy
import bmesh
from mathutils import Vector, Quaternion

ROOT=Path(__file__).resolve().parents[4]
BUNDLE=Path(os.environ.get('FRANK_OUTPUT_DIR',str(Path(bpy.data.filepath).parent)))
GLB_PATH=BUNDLE/'frank.glb'
if not GLB_PATH.exists():GLB_PATH=ROOT/'apps/frank_desktop/assets/character_model/frank.glb'
QA=BUNDLE/'qa';QA.mkdir(parents=True,exist_ok=True)
MASTER=Path(bpy.data.filepath)

def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()

def topology(mesh,weld=False):
    bm=bmesh.new();bm.from_mesh(mesh)
    if weld:bmesh.ops.remove_doubles(bm,verts=list(bm.verts),dist=1e-7)
    bm.verts.ensure_lookup_table()
    visited=set();components=0
    for vertex in bm.verts:
        if vertex in visited:continue
        components+=1;queue=[vertex];visited.add(vertex)
        while queue:
            for edge in queue.pop().link_edges:
                for neighbor in edge.verts:
                    if neighbor not in visited:visited.add(neighbor);queue.append(neighbor)
    result={'components':components,'nonmanifold_edges':sum(not e.is_manifold for e in bm.edges),
            'inconsistent_winding_edges':sum(e.is_manifold and not e.is_contiguous for e in bm.edges),
            'degenerate_faces':sum(f.calc_area()<1e-12 for f in bm.faces),
            'signed_volume':bm.calc_volume(signed=True)}
    bm.free();return result

def inspect(objects):
    meshes=[o for o in objects if o.type=='MESH'];rigs=[o for o in objects if o.type=='ARMATURE']
    triangles=0;max_influences=0;unweighted=0;sum_error=0
    for obj in meshes:
        obj.data.calc_loop_triangles();triangles+=len(obj.data.loop_triangles)
        for vertex in obj.data.vertices:
            active=[g.weight for g in vertex.groups if g.weight>1e-8]
            max_influences=max(max_influences,len(active));unweighted+=not active
            sum_error=max(sum_error,abs(sum(active)-1))
    corners=[obj.matrix_world@Vector(p) for obj in meshes for p in obj.bound_box]
    return {'meshes':len(meshes),'triangles':triangles,'armatures':len(rigs),
            'bones':sorted(b.name for b in rigs[0].data.bones) if len(rigs)==1 else [],
            'max_influences':max_influences,'unweighted':unweighted,'weight_sum_error':sum_error,
            'height_m':max(p.z for p in corners)-min(p.z for p in corners),
            'floor_z':min(p.z for p in corners),
            'finite_vertices':all(math.isfinite(v) for o in meshes for p in o.data.vertices for v in p.co),
            'cameras':sum(o.type=='CAMERA' for o in objects),'lights':sum(o.type=='LIGHT' for o in objects)}

character=bpy.data.collections['Frank_Character'];master=inspect(list(character.all_objects))
body=bpy.data.objects['Frank_Body'];body_topology=topology(body.data)
# Geometric section segments distinguish the three separated finger branches.
def section_components(z,side):
    segments=[]
    for poly in body.data.polygons:
        points=[body.data.vertices[i].co for i in poly.vertices]
        if any(side*p.x<.30 for p in points):continue
        hits=[]
        for a,b in zip(points,points[1:]+points[:1]):
            if (a.z-z)*(b.z-z)<0:
                p=a.lerp(b,(z-a.z)/(b.z-a.z));hits.append(tuple(round(v,6) for v in p))
        if len(hits)==2:segments.append(hits)
    graph={}
    for a,b in segments:graph.setdefault(a,set()).add(b);graph.setdefault(b,set()).add(a)
    count=0
    while graph:
        count+=1;queue=[next(iter(graph))]
        while queue:
            queue.extend(graph.pop(queue.pop(),()))
    return count
finger_sections={side:section_components(.36,sign) for side,sign in [('L',1),('R',-1)]}
# Bounds and connected contact on actual sole vertices, separately for both feet.
soles={}
for side,sign in [('L',1),('R',-1)]:
    points=[v.co for v in body.data.vertices if sign*v.co.x>.04 and v.co.z<.001]
    soles[side]={'vertices':len(points),'width':max(p.x for p in points)-min(p.x for p in points) if points else 0,
                 'depth':max(p.y for p in points)-min(p.y for p in points) if points else 0}
report={'asset_hashes':{'blend':sha(MASTER),'glb':sha(GLB_PATH)},'master':master,'body':body_topology,
        'three_finger_sections':finger_sections,'sole_contact':soles,
        'visual_status':'requires separate visual-review.json; technical success is not likeness'}
report['master_mesh_topology']={o.name:topology(o.data) for o in character.all_objects if o.type=='MESH'}
# Read the wire asset itself, then import into an empty scene rather than the master.
data=GLB_PATH.read_bytes();length,kind=struct.unpack_from('<II',data,12);gltf=json.loads(data[20:20+length])
report['glb_contract']={'skins':len(gltf.get('skins',[])),'images':len(gltf.get('images',[])),
    'embedded_images':all('bufferView' in i for i in gltf.get('images',[])),
    'animations':len(gltf.get('animations',[])),'bytes':len(data)}
expected_bones=master['bones']
rig=bpy.data.objects['Frank_Rig'];curl_directions={}
for side,sign in [('L',1),('R',-1)]:
 bone=rig.data.bones['fingers.'+side];axis=bone.matrix_local.to_3x3()@Vector((1,0,0))
 direction=bone.tail_local-bone.head_local
 curled=Quaternion(axis,math.radians(55))@direction
 curl_directions[side]=sign*(curled.x-direction.x)<0
report['positive_finger_flexion_inward']=curl_directions
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(GLB_PATH))
helpers={bone.custom_shape for obj in bpy.context.scene.objects if obj.type=='ARMATURE' for bone in obj.pose.bones if bone.custom_shape is not None}
report['importer_only_helpers']=[o.name for o in helpers]
exported=inspect([o for o in bpy.context.scene.objects if o not in helpers]);report['glb_reimport']=exported
report['glb_mesh_topology']={o.name:topology(o.data,True) for o in bpy.context.scene.objects if o.type=='MESH' and o not in helpers}
# glTF may split a mesh by material; evaluate joined skin topology after seam welding.
skins=[o for o in bpy.context.scene.objects if o.type=='MESH' and o.name.startswith('Frank_Body')]
bm=bmesh.new()
for obj in skins:bm.from_mesh(obj.data)
joined=bpy.data.meshes.new('Validation_Joined_Skin');bm.to_mesh(joined);bm.free()
report['glb_body']=topology(joined,True)
checks={'finger_flexion_toward_palm':all(curl_directions.values()),'single_closed_body':body_topology['components']==1 and body_topology['nonmanifold_edges']==0,
        'clean_body_faces':body_topology['degenerate_faces']==0 and body_topology['inconsistent_winding_edges']==0 and body_topology['signed_volume']>0,
        'three_fingers_per_hand':all(n==3 for n in finger_sections.values()),
        'flat_soles':all(s['width']>.08 and s['depth']>.10 for s in soles.values()),
        'one_23_bone_rig':master['armatures']==1 and len(expected_bones)==23,
        'height_and_floor':abs(master['height_m']-1.5)<.01 and abs(master['floor_z'])<.001,
        'glb_rig_preserved':exported['armatures']==1 and exported['bones']==expected_bones,
        'glb_height_and_floor':abs(exported['height_m']-1.5)<.01 and abs(exported['floor_z'])<.001,
        'glb_closed_body':report['glb_body']['components']==1 and report['glb_body']['nonmanifold_edges']==0,
        'embedded_textures':report['glb_contract']['embedded_images'] and report['glb_contract']['images']>0,
        'runtime_contract':not exported['cameras'] and not exported['lights'] and not report['glb_contract']['animations'] and len(data)<=20*1024*1024}
for name,stats in [('master',master),('glb',exported)]:
    checks[name+'_budget']=stats['triangles']<=30000
    checks[name+'_weights']=stats['max_influences']<=4 and stats['unweighted']==0 and stats['weight_sum_error']<1e-5
    checks[name+'_finite']=stats['finite_vertices']
checks['all_master_meshes_clean']=all(t['nonmanifold_edges']==0 and t['degenerate_faces']==0 and t['inconsistent_winding_edges']==0 and t['signed_volume']>0 for t in report['master_mesh_topology'].values())
checks['all_glb_meshes_clean']=all(t['degenerate_faces']==0 and t['inconsistent_winding_edges']==0 and t['signed_volume']>0 for t in report['glb_mesh_topology'].values())
report['checks']=checks;report['technical_passed']=all(checks.values())
(QA/'validation.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
if not report['technical_passed']:raise RuntimeError('Frank technical validation failed')
