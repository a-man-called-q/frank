"""Authored section cages for the 1.4 head and pinnae; no primitive deformation."""
import math
import bpy
import bmesh


def mesh_object(name, vertices, faces, collection, material, subdivisions=1):
    mesh=bpy.data.meshes.new(name+'_Cage');mesh.from_pydata(vertices,[],faces);mesh.update()
    bm=bmesh.new();bm.from_mesh(mesh);bmesh.ops.recalc_face_normals(bm,faces=list(bm.faces));bm.to_mesh(mesh);bm.free()
    obj=bpy.data.objects.new(name,mesh);collection.objects.link(obj);mesh.materials.append(material)
    bpy.context.view_layer.objects.active=obj;obj.select_set(True)
    if subdivisions:
        mod=obj.modifiers.new('Contour_Subdivision','SUBSURF');mod.levels=subdivisions
        bpy.ops.object.modifier_apply(modifier=mod.name)
    # These parts are rigidly bound to head: retain a smooth sampled silhouette
    # while reducing redundant interior triangles, without touching body weights.
    reduce=obj.modifiers.new('Rigid_Contour_Budget','DECIMATE');reduce.ratio=.25
    reduce.use_collapse_triangulate=True
    bpy.ops.object.modifier_apply(modifier=reduce.name)
    for face in obj.data.polygons:face.use_smooth=True
    obj.data.uv_layers.new(name='Painted_Projection');obj.select_set(False)
    return obj


def head(collection,material):
    # z, half width, front depth, rear depth. Crown and jaw have distinct curvature.
    sections=[(.870,.205,.150,.155),(.878,.249,.188,.191),(.898,.250,.202,.208),
              (.955,.265,.216,.220),(1.080,.279,.221,.232),(1.250,.290,.224,.240),
              (1.400,.294,.220,.237),(1.465,.290,.213,.229),(1.488,.260,.187,.202),
              (1.500,.175,.128,.138)]
    vertices=[];faces=[];n=24
    for z,rx,front,back in sections:
        for i in range(n):
            a=2*math.pi*i/n;c=math.cos(a);s=math.sin(a)
            x=rx*math.copysign(abs(c)**.43,c)
            y=(back if s>=0 else front)*math.copysign(abs(s)**.43,s)
            vertices.append((x,y,z-1.185))
    for j in range(len(sections)-1):
        for i in range(n):faces.append((j*n+i,j*n+(i+1)%n,(j+1)*n+(i+1)%n,(j+1)*n+i))
    faces.append(tuple(reversed(range(n))));faces.append(tuple((len(sections)-1)*n+i for i in range(n)))
    obj=mesh_object('Head',vertices,faces,collection,material,2);obj.location.z=1.185
    return obj


def ear(side,sign,collection,material):
    # Outline in local width/height; embedded medial edge, broad upper rim, soft lobe.
    outline=[(-.030,.062),(-.011,.089),(.024,.090),(.053,.072),(.066,.040),
             (.067,.004),(.054,-.037),(.031,-.070),(.005,-.078),(-.018,-.061),
             (-.025,-.028),(-.015,.007),(-.026,.031)]
    vertices=[];faces=[];n=len(outline)
    # Closed lenticular pinna: no geometric inner grooves, those remain painted.
    for scale,depth in [(0.20,-.017),(.72,-.024),(1.0,-.003),(.86,.025),(.20,.009)]:
        for w,z in outline:
            w*=scale*1.15;z*=scale
            vertices.append((sign*(.81*w+.59*depth), .59*w-.81*depth,z))
    for j in range(4):
        for i in range(n):faces.append((j*n+i,j*n+(i+1)%n,(j+1)*n+(i+1)%n,(j+1)*n+i))
    faces.append(tuple(reversed(range(n))));faces.append(tuple(4*n+i for i in range(n)))
    obj=mesh_object('Ear.'+side,vertices,faces,collection,material,2);obj.location=(sign*.279,-.023,1.083)
    uv=obj.data.uv_layers.active
    for loop in obj.data.loops:
        co=obj.data.vertices[loop.vertex_index].co;w=.81*sign*co.x+.59*co.y
        uv.data[loop.index].uv=((w+.035)/.110,(co.z+.085)/.185)
    return obj
