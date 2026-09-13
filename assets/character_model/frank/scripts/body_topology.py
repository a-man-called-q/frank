"""Deterministic connected control cage. Branches share boundary vertices.

No voxel union, boolean, spatial weld, or decimation participates in skin creation.
Weights are authored on anatomical rings and interpolated by Catmull-Clark.
"""
import math
import bpy
import bmesh
from mathutils import Vector


class Cage:
    def __init__(self):
        self.vertices=[]; self.faces=[]; self.materials=[]; self.weights=[]
    def vertex(self, co, weights):
        self.vertices.append(tuple(co)); self.weights.append(weights.copy())
        return len(self.vertices)-1
    def face(self, ids, material=1):
        self.faces.append(tuple(ids)); self.materials.append(material)
    def bridge(self, a, b, material=1):
        # Equal loops produce quads; a change in ring density uses local triangles.
        i=j=0
        while i<len(a) or j<len(b):
            na=(i+1)/len(a) if i<len(a) else 2
            nb=(j+1)/len(b) if j<len(b) else 2
            if abs(na-nb)<1e-8:
                self.face((a[i%len(a)],a[(i+1)%len(a)],b[(j+1)%len(b)],b[j%len(b)]),material);i+=1;j+=1
            elif na<nb:
                self.face((a[i%len(a)],a[(i+1)%len(a)],b[j%len(b)]),material);i+=1
            else:
                self.face((a[i%len(a)],b[(j+1)%len(b)],b[j%len(b)]),material);j+=1
    def ring(self, center, u, v, rx, ry, n, weights, offset=0):
        center,u,v=Vector(center),Vector(u),Vector(v)
        return [self.vertex(center+u*(rx*math.cos(offset+2*math.pi*i/n))+v*(ry*math.sin(offset+2*math.pi*i/n)),weights) for i in range(n)]
    def cap(self, ring, co, weights, material=1):
        tip=self.vertex(co,weights)
        for a,b in zip(ring,ring[1:]+ring[:1]):self.face((a,b,tip),material)


def blend(a,b,t):
    t=max(0,min(1,t));t=t*t*(3-2*t)
    return {k:v for k,v in ((a,1-t),(b,t)) if v>1e-8}


def build_body(collection, skin):
    c=Cage()
    # Bottom loop remains open and splits into two leg openings over a shared saddle.
    sections=[(.435,.178,.108),(.48,.179,.116),(.525,.180,.120),(.60,.180,.122),
              (.70,.181,.120),(.775,.177,.106),(.850,.160,.090),
              (.885,.085,.070),(.901,.071,.065)]
    rings=[]
    for z,rx,ry in sections:
        weights=blend('pelvis','spine',(z-.49)/.19) if z<.79 else blend('spine','neck',(z-.82)/.10)
        # Independent anterior/posterior contour; a gentle belly and full seat.
        front={.435:.133,.48:.155,.525:.166,.60:.164,.70:.153,.775:.130,.850:.102,.885:.074,.901:.065}[z]
        back={.435:.146,.48:.160,.525:.153,.60:.143,.70:.137,.775:.118,.850:.095,.885:.073,.901:.065}[z]
        ring=c.ring((0,0,z),(1,0,0),(0,1,0),rx,ry,16,weights)
        for index in ring:
            x,y,h=c.vertices[index];c.vertices[index]=(x,y*(front if y<0 else back)/ry,h)
        rings.append(ring)
    for j in range(len(rings)-1):
        for i in range(16):
            if j in (4,5) and i in (15,0,7,8):continue
            c.face((rings[j][i],rings[j][(i+1)%16],rings[j+1][(i+1)%16],rings[j+1][i]))
    # Vertices internal to removed shoulder patches are removed on final compaction.
    c.cap(rings[-1],(0,0,.945),{'neck':1})
    bottom=rings[0]
    saddle=[]
    for j in range(1,8):
        t=j/8
        saddle.append(c.vertex((0,.139*(1-2*t),.435-.055*math.sin(math.pi*t)),{'pelvis':1}))
    for sign,side in ((1,'L'),(-1,'R')):
        suffix='.'+side
        if sign==1:loop=[bottom[i%16] for i in range(12,21)]+saddle
        else:loop=[bottom[i%16] for i in range(12,3,-1)]+saddle
        # Both leg loops start at the front inner point and run around the outer thigh.
        for z,x,rx,ry,cy in [(.401,.102,.079,.090,0),
                             (.315,.117,.073,.079,.002),
                             (.230,.130,.062,.066,.003),
                             (.145,.136,.061,.065,0),(.102,.137,.060,.065,-.005),
                             (.072,.137,.078,.094,-.028),(.040,.137,.084,.112,-.040),
                             (.006,.137,.070,.102,-.041),
                             (.006,.137,.044,.066,-.041)]:
            if z>=.315:w=blend('pelvis','thigh'+suffix,(.445-z)/.085)
            elif z>=.145:w=blend('thigh'+suffix,'shin'+suffix,(.285-z)/.105)
            else:w=blend('shin'+suffix,'foot'+suffix,(.135-z)/.065)
            new=c.ring((sign*x,cy,z),(sign,0,0),(0,1,0),rx,ry,16,w,-math.pi/2)
            if z<.102:
                for index in new:
                    toe=max(0,min(1,(-c.vertices[index][1]-.070)/.065))
                    toe=toe*toe*(3-2*toe)
                    amount=c.weights[index].get('foot'+suffix,0)
                    if amount*toe>0:
                        c.weights[index]['foot'+suffix]=amount*(1-toe)
                        c.weights[index]['toes'+suffix]=amount*toe
            if abs(z-.401)<1e-6:
                # The inner thigh descends away from the saddle instead of
                # rising above it and leaving a small hanging center point.
                for i,index in enumerate(new):
                    x0,y0,z0=c.vertices[index]
                    inward=max(0,-math.cos(-math.pi/2+2*math.pi*i/16))
                    c.vertices[index]=(x0,y0,z0-.027*inward**4)
            c.bridge(loop,new);loop=new
        c.cap(loop,(sign*.137,-.041,.006),{'foot'+suffix:1})
        # Shoulder patch boundary: 2 by 2 quad patch, eight boundary vertices.
        cols=[15,0,1] if sign==1 else [9,8,7]
        loop=[rings[4][cols[0]],rings[4][cols[1]],rings[4][cols[2]],rings[5][cols[2]],
              rings[6][cols[2]],rings[6][cols[1]],rings[6][cols[0]],rings[5][cols[0]]]
        for idx in loop:
            x,y,z=c.vertices[idx]
            if abs(y)>1e-5:c.vertices[idx]=(x,y*1.35,z)
            c.weights[idx]=blend('spine','upper_arm'+suffix,.25)
        # The u axis is palm width (depth); v is hand thickness (outward/upward).
        u=(0,1,0);v=(sign*.88,0,.475)
        arm_sections=[(.193,.775,.061,.066),(.221,.731,.060,.063),
                      (.252,.674,.056,.057),(.270,.628,.055,.054),
                      (.287,.591,.052,.052),(.318,.537,.046,.046),
                      (.344,.486,.040,.040),(.357,.461,.047,.032),
                      (.368,.432,.056,.029),(.372,.407,.057,.027)]
        hand_loops=[]
        for j,(x,z,ru,rv) in enumerate(arm_sections):
            if j<2:w=blend('shoulder'+suffix,'upper_arm'+suffix,.75+j*.25)
            elif z>=.537:w=blend('upper_arm'+suffix,'forearm'+suffix,(.675-z)/.09)
            else:w=blend('forearm'+suffix,'hand'+suffix,(.52-z)/.07)
            new=c.ring((sign*x,0,z),u,v,ru,rv,8,w,-3*math.pi/4)
            # Thumb opening on the inward/front quarter of the palm.
            if j==8:
                for i in range(8):
                    if i in (0,1):continue
                    c.face((loop[i],loop[(i+1)%8],new[(i+1)%8],new[i]),0)
                thumb_loop=[loop[0],loop[1],loop[2],new[2],new[1],new[0]]
            else:c.bridge(loop,new,0)
            loop=new;hand_loops.append(new)
        # Flattened palm perimeter; shared cross-palm webs split it into 3 fingers.
        # Start is front/inward, following the same ordering as the forearm rings.
        xy=[(-.024,-.058),(0,-.058),(.024,-.058),(.024,-.019),(.024,.019),
            (.024,.058),(0,.058),(-.024,.058),(-.024,.019),(-.024,-.019)]
        perimeter=[c.vertex((sign*(.377+x),y,.387),{'hand'+suffix:1}) for x,y in xy]
        # Match the old ring by nearest angular start before changing density.
        # Ring index zero lies front/inward; reverse ordering matches palm perimeter.
        c.bridge(loop,[perimeter[0]]+list(reversed(perimeter[1:])),0)
        web1=c.vertex((sign*.377,-.019,.387),{'hand'+suffix:1})
        web2=c.vertex((sign*.377,.019,.387),{'hand'+suffix:1})
        p=perimeter
        openings=[[p[0],p[1],p[2],p[3],web1,p[9]],
                  [p[9],web1,p[3],p[4],web2,p[8]],
                  [p[8],web2,p[4],p[5],p[6],p[7]]]
        for k,(opening,y,length) in enumerate(zip(openings,(-.038,0,.038),(.057,.066,.054))):
            previous=opening
            # Six-sided elliptical loops align to the rectangular opening corners.
            angles=[-2.36,-1.57,-.785,.785,1.57,2.36]
            for t,rx,ry in ((.16,.024,.015),(.60,.023,.018),(1,.017,.014)):
                z=.386-length*t;x=.380+.024*t+(-.009,0,.006)[k]*t
                w=blend('hand'+suffix,'fingers'+suffix,t)
                new=[c.vertex((sign*(x+rx*math.cos(a)),y+ry*math.sin(a),z),w) for a in angles]
                # Opening order is inward/front -> outward/front -> outward/back.
                c.bridge(previous,new,0);previous=new
            c.cap(previous,(sign*(.405+(-.009,0,.006)[k]),y,.380-length),{'fingers'+suffix:1},0)
        # Thumb leaves an actual palm opening; it is not an intersecting capsule.
        previous=thumb_loop
        center=sum((Vector(c.vertices[i]) for i in previous),Vector())/len(previous)
        end=Vector((sign*.300,-.028,.391));axis=(end-center).normalized()
        tu=Vector((0,1,0));tv=axis.cross(tu).normalized();tu=tv.cross(axis).normalized()
        angles=[math.atan2((Vector(c.vertices[i])-center).dot(tv),(Vector(c.vertices[i])-center).dot(tu)) for i in previous]
        # Keep boundary correspondence, avoiding a rotated bridge.
        for t,r in ((.35,.027),(.72,.024),(1,.016)):
            cc=center.lerp(end,t)
            new=[c.vertex(cc+tu*(r*math.cos(a))+tv*(r*math.sin(a)),{'hand'+suffix:1}) for a in angles]
            c.bridge(previous,new,0);previous=new
        c.cap(previous,end+axis*.009,{'hand'+suffix:1},0)
    used=sorted({i for f in c.faces for i in f});mapping={old:i for i,old in enumerate(used)}
    mesh=bpy.data.meshes.new('Frank_Control_Cage');mesh.from_pydata([c.vertices[i] for i in used],[],[[mapping[i] for i in f] for f in c.faces]);mesh.update()
    obj=bpy.data.objects.new('Frank_Body',mesh);collection.objects.link(obj)
    mesh.materials.append(skin);mesh.materials.append(skin)
    for p,material in zip(mesh.polygons,c.materials):p.material_index=material
    groups={name:obj.vertex_groups.new(name=name) for name in sorted({n for i in used for n in c.weights[i]})}
    for index,old in enumerate(used):
        for name,weight in c.weights[old].items():groups[name].add([index],weight,'REPLACE')
    bm=bmesh.new();bm.from_mesh(mesh);bmesh.ops.recalc_face_normals(bm,faces=list(bm.faces));bm.to_mesh(mesh);bm.free()
    bpy.context.view_layer.objects.active=obj;obj.select_set(True)
    sub=obj.modifiers.new('Authored_Surface','SUBSURF');sub.levels=2;sub.render_levels=2
    bpy.ops.object.modifier_apply(modifier=sub.name)
    # Simplify only rigidly weighted patches, never interpolate across joint loops.
    bm=bmesh.new();bm.from_mesh(mesh);deform=bm.verts.layers.deform.active
    def rigid_key(v):
        items=[(k,w) for k,w in v[deform].items() if w>1e-6]
        return items[0][0] if len(items)==1 and items[0][1]>.999 else None
    eligible=[]
    for edge in bm.edges:
        vertices={v for f in edge.link_faces for v in f.verts}
        keys={rigid_key(v) for v in vertices}
        if len(keys)==1 and None not in keys:eligible.append(edge)
    bmesh.ops.dissolve_limit(bm,angle_limit=.045,verts=[],edges=eligible,delimit={'MATERIAL'})
    # Fixed rest triangulation avoids pose-dependent tessellation of large n-gons.
    bmesh.ops.triangulate(bm,faces=[f for f in bm.faces if len(f.verts)>4])
    bm.to_mesh(mesh);bm.free()
    minimum=min(v.co.z for v in mesh.vertices)
    for vertex in mesh.vertices:vertex.co.z-=minimum
    for p in mesh.polygons:p.use_smooth=True
    obj['fingers_per_hand']=4;obj['toes_per_foot']=0;obj['construction']='Connected authored control cage; shared edge-loop branches'
    obj.select_set(False)
    return obj
