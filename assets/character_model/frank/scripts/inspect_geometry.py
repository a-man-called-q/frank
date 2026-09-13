"""Read-only shape/weight inspection; prints JSON for automated capture or MCP."""
import bpy
import bmesh
import json
import hashlib
from pathlib import Path
from mathutils import Vector

body=bpy.data.objects['Frank_Body']

def cross_sections(z):
    mesh=body.data
    points={};adj={}
    for edge in mesh.edges:
        a=mesh.vertices[edge.vertices[0]].co;b=mesh.vertices[edge.vertices[1]].co
        if (a.z-z)*(b.z-z)<0:
            key=tuple(sorted(edge.vertices))
            points[key]=a.lerp(b,(z-a.z)/(b.z-a.z));adj[key]=[]
    for poly in mesh.polygons:
        keys=[tuple(sorted(k)) for k in poly.edge_keys if tuple(sorted(k)) in points]
        if len(keys)==2:
            adj[keys[0]].append(keys[1]);adj[keys[1]].append(keys[0])
    components=[];seen=set()
    for seed in points:
        if seed in seen:continue
        stack=[seed];group=[]
        while stack:
            key=stack.pop()
            if key in seen:continue
            seen.add(key);group.append(points[key]);stack.extend(adj[key])
        components.append(group)
    return components

measure={}
for name,z,target_x in [('chest_width',.62,0),('waist_width',.525,0),('hip_width',.47,0),('hand_width',.42,.38),('foot_width',.03,.13)]:
    best=[];distance=100
    for component in cross_sections(z):
        center=sum(p.x for p in component)/len(component)
        if abs(center-target_x)<distance:
            best=component;distance=abs(center-target_x)
    measure[name]=max(p.x for p in best)-min(p.x for p in best)
    if name=='chest_width':measure['chest_depth']=max(p.y for p in best)-min(p.y for p in best)

meshes=[o for o in bpy.data.collections['Frank_Character'].objects if o.type=='MESH']
triangles=0;maxweights=0;unweighted=0;weight_error=0
for obj in meshes:
    obj.data.calc_loop_triangles();triangles+=len(obj.data.loop_triangles)
    for vertex in obj.data.vertices:
        weights=[g.weight for g in vertex.groups]
        maxweights=max(maxweights,len(weights));unweighted+=not weights
        weight_error=max(weight_error,abs(sum(weights)-1))
bm=bmesh.new();bm.from_mesh(body.data)
nonmanifold=sum(not e.is_manifold for e in bm.edges)
# Connectivity: touching separate pieces are not accepted as a continuous body.
seen=set();components=0
for v in bm.verts:
    if v in seen:continue
    components+=1;stack=[v]
    while stack:
        item=stack.pop()
        if item in seen:continue
        seen.add(item);stack.extend(e.other_vert(item) for e in item.link_edges)
bm.free()
points=[obj.matrix_world@Vector(c) for obj in meshes for c in obj.bound_box]
sole={}
for side,sign in [('L',1),('R',-1)]:
    points_side=[v.co for v in body.data.vertices if sign*v.co.x>0 and v.co.z<.0001]
    sole[side]={'vertices_within_0_1mm':len(points_side),'width':max(p.x for p in points_side)-min(p.x for p in points_side),'depth':max(p.y for p in points_side)-min(p.y for p in points_side)}
report={'measurements_m':measure,'triangles':triangles,'mesh_count':len(meshes),'height_m':max(p.z for p in points)-min(p.z for p in points),'body_nonmanifold_edges':nonmanifold,'body_components':components,'max_influences':maxweights,'unweighted':unweighted,'max_weight_sum_error':weight_error,'sole_contact':sole,'rig_bones':len(bpy.data.objects['Frank_Rig'].data.bones)}
print('FRANK_GEOMETRY='+json.dumps(report))

# Comparable dimensions with explicit measurement definitions and fixed targets.
ref=json.loads((Path(__file__).resolve().parents[1]/'reference-calibration.json').read_text())
scale=1.5/(ref['pixel_floor']-ref['pixel_top'])
hand=[v.co for v in body.data.vertices if v.co.x>.27 and .31<v.co.z<.46]
foot=[v.co for v in body.data.vertices if v.co.x>.04 and v.co.z<.10]
head=bpy.data.objects['Head'];head_points=[head.matrix_world@v.co for v in head.data.vertices]
values={'head':max(v.x for v in head_points)-min(v.x for v in head_points),
        'chest':measure['chest_width'],'waist':measure['waist_width'],'hip':measure['hip_width'],
        'foot':max(p.x for p in foot)-min(p.x for p in foot),
        'hand':max(p.x for p in hand)-min(p.x for p in hand)}
measurements={}
for name,value in values.items():
 target=ref['widths_px'][name]*scale
 measurements[name]={'target_m':target,'measured_m':value,'error_percent':abs(value-target)/target*100}
master=Path(bpy.data.filepath)
report['reference_dimensions']=measurements
report['dimension_target_passed']=all(v['error_percent']<=5 for v in measurements.values())
report['measurement_definitions']={'head':'maximum world X width','chest':'central contour at z=.62',
 'waist':'central contour at z=.525','hip':'central contour at z=.47',
 'foot':'full forefoot X extent below z=.10','hand':'full hand X extent between z=.31 and .46, x>.27'}
profiles={}
for row,(left,right) in ref['contour_rows_px'].items():
 z=(ref['pixel_floor']-int(row))*scale
 components=cross_sections(z)
 contour=min(components,key=lambda pts:abs(sum(p.x for p in pts)/len(pts)))
 width=max(p.x for p in contour)-min(p.x for p in contour);target=(right-left)*scale
 profiles[row]={'z_m':z,'target_width_m':target,'measured_width_m':width,'error_percent':abs(width-target)/target*100}
report['front_contour_profiles']=profiles
rig=bpy.data.objects['Frank_Rig']
landmark_values={'head_bottom':min(p.z for p in head_points),'elbow':rig.data.bones['forearm.L'].head_local.z,
 'wrist':rig.data.bones['hand.L'].head_local.z,'brief_top':.525,'knee':rig.data.bones['shin.L'].head_local.z}
report['front_landmarks']={}
for name,value in landmark_values.items():
 target=(ref['pixel_floor']-ref['landmarks_px'][name])*scale
 report['front_landmarks'][name]={'target_z_m':target,'measured_z_m':value,'error_percent':abs(value-target)/target*100}
report['profile_target_passed']=all(v['error_percent']<=5 for v in profiles.values())
report['landmark_target_passed']=all(v['error_percent']<=5 for v in report['front_landmarks'].values())
calibration14=master.parent/'qa/reference-calibration-1.4.json'
if calibration14.exists():
    ref14=json.loads(calibration14.read_text());head=bpy.data.objects['Head'];profiles14={}
    for row,(left,right) in ref14['head_front_rows_px'].items():
        z=(ref14['pixel_floor']-int(row))*scale;hits=[]
        for edge in head.data.edges:
            a,b=[head.matrix_world@head.data.vertices[i].co for i in edge.vertices]
            if (a.z-z)*(b.z-z)<0:hits.append(a.lerp(b,(z-a.z)/(b.z-a.z)).x)
        measured=max(hits)-min(hits);target=(right-left)*scale
        profiles14[row]={'z_m':z,'measured_width_m':measured,'target_width_m':target,'error_percent':abs(measured-target)/target*100}
    report['head_front_profiles']=profiles14
    report['head_profile_target_passed']=all(v['error_percent']<=5 for v in profiles14.values())
    report['torso_depth_profile']={}
    for z in (.47,.525,.60,.70,.775):
        contour=min(cross_sections(z),key=lambda pts:abs(sum(p.x for p in pts)/len(pts)))
        report['torso_depth_profile'][str(z)]={'front_y':min(p.y for p in contour),'back_y':max(p.y for p in contour)}
report['blend_sha256']=hashlib.sha256(master.read_bytes()).hexdigest()
(master.parent/'qa/geometry-validation.json').write_text(json.dumps(report,indent=2)+'\n')
