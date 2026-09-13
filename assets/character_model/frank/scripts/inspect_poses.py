"""Pose QA with localized stretch evidence; restores the original rig in all cases."""
import bpy
import math
import json
import hashlib
from pathlib import Path

rig=bpy.data.objects['Frank_Rig'];body=bpy.data.objects['Frank_Body']
saved={b.name:(b.matrix_basis.copy(),b.rotation_mode) for b in rig.pose.bones}
rest=[v.co.copy() for v in body.data.vertices]
lengths=[(rest[e.vertices[0]]-rest[e.vertices[1]]).length for e in body.data.edges]
POSES={
 'elbows_knees_90':{'forearm.L':(90,0,0),'forearm.R':(90,0,0),'shin.L':(-90,0,0),'shin.R':(-90,0,0)},
 'shoulders':{'upper_arm.L':(0,-25,-55),'upper_arm.R':(0,25,55),'head':(0,0,28)},
 'grip':{'hand.L':(0,-20,-15),'hand.R':(0,20,15),'fingers.L':(55,0,0),'fingers.R':(55,0,0)},
 'sit':{'thigh.L':(-82,0,0),'thigh.R':(-82,0,0),'shin.L':(92,0,0),'shin.R':(92,0,0),'spine':(8,0,0)},
}
report={}
try:
 for name,rotations in POSES.items():
  for bone in rig.pose.bones:bone.matrix_basis.identity();bone.rotation_mode='XYZ'
  for key,rotation in rotations.items():rig.pose.bones[key].rotation_euler=tuple(math.radians(v) for v in rotation)
  bpy.context.view_layer.update()
  obj=body.evaluated_get(bpy.context.evaluated_depsgraph_get());mesh=obj.to_mesh()
  edges=[]
  for edge,original in zip(body.data.edges,lengths):
   a,b=edge.vertices;length=(mesh.vertices[a].co-mesh.vertices[b].co).length
   if original>1e-6:edges.append({'index':edge.index,'ratio':length/original,'rest_length_m':original,'extension_m':length-original,
     'rest_midpoint':list((rest[a]+rest[b])/2),'posed_midpoint':list((mesh.vertices[a].co+mesh.vertices[b].co)/2)})
  edges.sort(key=lambda e:e['ratio'])
  report[name]={'maximum_edge_stretch':edges[-1]['ratio'],'p99_edge_stretch':edges[int(len(edges)*.99)]['ratio'],
                'worst_edges':list(reversed(edges[-12:])),'finite_vertices':all(math.isfinite(c) for v in mesh.vertices for c in v.co)}
  obj.to_mesh_clear()
finally:
 for bone in rig.pose.bones:
  matrix,mode=saved[bone.name];bone.rotation_mode=mode;bone.matrix_basis=matrix
 bpy.context.view_layer.update()
master=Path(bpy.data.filepath)
result={'blend_sha256':hashlib.sha256(master.read_bytes()).hexdigest(),'poses':report,
        'review_required':'Inspect the reported locations and pose renders; ratios alone do not establish visual acceptance.'}
(master.parent/'qa/pose-validation.json').write_text(json.dumps(result,indent=2)+'\n')
print('FRANK_POSES='+json.dumps({k:{f:v[f] for f in ('maximum_edge_stretch','p99_edge_stretch','finite_vertices')} for k,v in report.items()}))
