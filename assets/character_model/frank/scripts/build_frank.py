"""Build the clean, rigged Frank base character in Blender 5.x.

Builds an original procedural base against the male_apose_gpt references.
No external body, scripts, or rig data are imported.
"""

from __future__ import annotations

import math
import os
import sys
import shutil
from pathlib import Path

import bpy
from mathutils import Matrix, Vector


ROOT = Path(__file__).resolve().parents[4]
SCRIPT_DIR = ROOT / "assets/character_model/frank/scripts"
sys.path.insert(0, str(SCRIPT_DIR))
SOURCE_DIR = Path(globals().get("FRANK_OUTPUT_DIR", os.environ.get("FRANK_OUTPUT_DIR", str(ROOT / "assets/character_model/frank/candidate-1.4"))))
TEXTURE_DIR = SOURCE_DIR / "textures"
QA_DIR = SOURCE_DIR / "qa"
BLEND_PATH = SOURCE_DIR / "frank.blend"
GLB_PATH = SOURCE_DIR / "frank.glb"

for directory in (SOURCE_DIR, TEXTURE_DIR, QA_DIR, GLB_PATH.parent):
    directory.mkdir(parents=True, exist_ok=True)

shutil.copy2(SCRIPT_DIR.parent / 'reference-calibration.json', QA_DIR / 'reference-calibration-1.4.json')


def clean_file() -> None:
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    for collection in list(bpy.data.collections):
        if collection.name != "Collection":
            bpy.data.collections.remove(collection)
    base = bpy.data.collections.get("Collection")
    if base is None:
        base = bpy.data.collections.new("Frank_Character")
        bpy.context.scene.collection.children.link(base)
    else:
        base.name = "Frank_Character"
    for datablocks in (
        bpy.data.meshes,
        bpy.data.curves,
        bpy.data.armatures,
        bpy.data.materials,
        bpy.data.images,
        bpy.data.cameras,
        bpy.data.lights,
        bpy.data.actions,
        bpy.data.texts,
    ):
        for datablock in list(datablocks):
            if getattr(datablock, "users", 0) == 0:
                datablocks.remove(datablock)


clean_file()
CHAR = bpy.data.collections.get("Frank_Character")
QA = bpy.data.collections.new("QA_Render_Only")
bpy.context.scene.collection.children.link(QA)


def move_to_collection(obj: bpy.types.Object, collection: bpy.types.Collection) -> None:
    for old in list(obj.users_collection):
        old.objects.unlink(obj)
    collection.objects.link(obj)


def material_with_texture(
    name: str,
    rgba: tuple[float, float, float, float],
    roughness: float,
    metallic: float = 0.0,
) -> bpy.types.Material:
    image = bpy.data.images.new(f"{name}_BaseColor", width=64, height=64, alpha=True)
    image.generated_color = rgba
    image.filepath_raw = str(TEXTURE_DIR / f"{name.lower()}_basecolor.png")
    image.file_format = "PNG"
    image.save()

    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    nodes = mat.node_tree.nodes
    nodes.clear()
    output = nodes.new("ShaderNodeOutputMaterial")
    shader = nodes.new("ShaderNodeBsdfPrincipled")
    texture = nodes.new("ShaderNodeTexImage")
    texture.image = image
    shader.inputs["Roughness"].default_value = roughness
    shader.inputs["Metallic"].default_value = metallic
    mat.node_tree.links.new(texture.outputs["Color"], shader.inputs["Base Color"])
    mat.node_tree.links.new(shader.outputs["BSDF"], output.inputs["Surface"])
    image.pack()
    return mat


SKIN = material_with_texture("Skin_Green", (0.47, 0.76, 0.17, 1.0), 0.68)
CHARCOAL = material_with_texture("Charcoal", (0.025, 0.032, 0.040, 1.0), 0.52)
CREAM = material_with_texture("Eye_Cream", (0.96, 0.89, 0.72, 1.0), 0.48)
WHITE = material_with_texture("Eye_Highlight", (1.0, 1.0, 0.98, 1.0), 0.35)
METAL = material_with_texture("Bolt_Metal", (0.46, 0.49, 0.52, 1.0), 0.24, 0.78)
UNDERWEAR = material_with_texture("Underwear_Charcoal", (0.075, 0.085, 0.095, 1.0), 0.72)


def finish_mesh(obj: bpy.types.Object, material: bpy.types.Material, smooth: bool = True) -> bpy.types.Object:
    obj.data.materials.append(material)
    if smooth:
        for polygon in obj.data.polygons:
            polygon.use_smooth = True
    move_to_collection(obj, CHAR)
    obj.select_set(False)
    return obj


def rounded_cube(
    name: str,
    location: tuple[float, float, float],
    half_size: tuple[float, float, float],
    material: bpy.types.Material,
    bevel: float = 0.025,
) -> bpy.types.Object:
    bpy.ops.object.select_all(action="DESELECT")
    bpy.ops.mesh.primitive_cube_add(location=location)
    obj = bpy.context.object
    obj.name = name
    obj.scale = half_size
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    bevel_mod = obj.modifiers.new("Soft_Edges", "BEVEL")
    bevel_mod.width = bevel
    bevel_mod.segments = 6
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.modifier_apply(modifier=bevel_mod.name)
    return finish_mesh(obj, material)


def sphere(
    name: str,
    location: tuple[float, float, float],
    scale: tuple[float, float, float],
    material: bpy.types.Material,
    segments: int = 24,
    rings: int = 12,
) -> bpy.types.Object:
    bpy.ops.object.select_all(action="DESELECT")
    bpy.ops.mesh.primitive_uv_sphere_add(segments=segments, ring_count=rings, location=location)
    obj = bpy.context.object
    obj.name = name
    obj.scale = scale
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    return finish_mesh(obj, material)


def cylinder_between(
    name: str,
    start: tuple[float, float, float],
    end: tuple[float, float, float],
    radius: float,
    material: bpy.types.Material,
    vertices: int = 16,
) -> bpy.types.Object:
    bpy.ops.object.select_all(action="DESELECT")
    a, b = Vector(start), Vector(end)
    direction = b - a
    bpy.ops.mesh.primitive_cylinder_add(vertices=vertices, radius=radius, depth=direction.length, location=(a + b) / 2)
    obj = bpy.context.object
    obj.name = name
    obj.rotation_mode = "QUATERNION"
    obj.rotation_quaternion = direction.to_track_quat("Z", "Y")
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    finish_mesh(obj, material)
    for polygon in obj.data.polygons:
        polygon.use_smooth = len(polygon.vertices) == 4
    return obj


def torus(
    name: str,
    location: tuple[float, float, float],
    major_radius: float,
    minor_radius: float,
    material: bpy.types.Material,
) -> bpy.types.Object:
    bpy.ops.object.select_all(action="DESELECT")
    bpy.ops.mesh.primitive_torus_add(
        major_radius=major_radius,
        minor_radius=minor_radius,
        major_segments=32,
        minor_segments=8,
        location=location,
        rotation=(math.pi / 2, 0, 0),
    )
    obj = bpy.context.object
    obj.name = name
    return finish_mesh(obj, material)


def tube_curve(
    name: str,
    points: list[tuple[float, float, float]],
    radius: float,
    material: bpy.types.Material,
) -> bpy.types.Object:
    bpy.ops.object.select_all(action="DESELECT")
    curve = bpy.data.curves.new(name, "CURVE")
    curve.dimensions = "3D"
    curve.resolution_u = 12
    curve.bevel_depth = radius
    curve.bevel_resolution = 2
    spline = curve.splines.new("BEZIER")
    spline.bezier_points.add(len(points) - 1)
    for point, coord in zip(spline.bezier_points, points):
        point.co = coord
        point.handle_left_type = "AUTO"
        point.handle_right_type = "AUTO"
    obj = bpy.data.objects.new(name, curve)
    CHAR.objects.link(obj)
    obj.data.materials.append(material)
    bpy.context.view_layer.objects.active = obj
    obj.select_set(True)
    bpy.ops.object.convert(target="MESH")
    obj.select_set(False)
    for polygon in obj.data.polygons:
        polygon.use_smooth = True
    return obj


# Authored topology owns anatomical regions and interpolated control weights.
parts_by_bone = {}
def bind_later(obj, bone_name):
    parts_by_bone.setdefault(bone_name, []).append(obj)
    return obj
import importlib
import body_topology
importlib.reload(body_topology)
BODY = body_topology.build_body(CHAR, SKIN)

# Brief is painted into the skin base-color map, without a clothing shell.
body_image=bpy.data.images.new('Body_Brief_BaseColor',width=1024,height=1024,alpha=True)
body_pixels=[]
for row in range(1024):
    z=(row+.5)/1024*1.5
    for col in range(1024):
        angle=((col+.5)/1024-.5)*2*math.pi
        bottom=.367+.092*abs(math.cos(angle))**1.5
        coverage=max(0,min(1,(z-bottom)/.0015+.5))*max(0,min(1,(.525-z)/.0015+.5))
        body_pixels.extend((.47*(1-coverage)+.075*coverage,.76*(1-coverage)+.085*coverage,.17*(1-coverage)+.095*coverage,1))
body_image.pixels.foreach_set(body_pixels)
body_image.filepath_raw=str(TEXTURE_DIR / 'body_brief_basecolor.png')
body_image.file_format='PNG'
body_image.save()
body_image.pack()
body_mat=SKIN.copy()
body_mat.name='Body_Painted_Brief'
for node in body_mat.node_tree.nodes:
    if node.type=='TEX_IMAGE': node.image=body_image
BODY.data.materials[1] = body_mat
if not BODY.data.uv_layers: BODY.data.uv_layers.new(name="UVMap")
uv=BODY.data.uv_layers.active
for poly in BODY.data.polygons:
    us=[]
    for li in poly.loop_indices:
        co=BODY.data.vertices[BODY.data.loops[li].vertex_index].co
        us.append(math.atan2(co.y,co.x)/(2*math.pi)+.5)
    seam=max(us)-min(us)>.5
    for li,u in zip(poly.loop_indices,us):
        co=BODY.data.vertices[BODY.data.loops[li].vertex_index].co
        uv.data[li].uv=((u+1 if seam and u<.5 else u),co.z/1.5) if poly.material_index==1 else (.5,.95)

from head_topology import head, ear as build_ear
HEAD=bind_later(head(CHAR,SKIN),'head')
face_mat=SKIN.copy(); face_mat.name='Face_Painted_Smile'
image=bpy.data.images.new('Face_Smile_BaseColor',width=1024,height=1024,alpha=True)
# Analytic antialiased stroke painted directly into the head base-color map.
pixels=[]
for row in range(1024):
    z=.87+(row+.5)/1024*.63
    for col in range(1024):
        x=((col+.5)/1024-.5)*.574
        curve=.966+2.4*x*x
        coverage=max(0,min(1,(.0024-abs(z-curve))/.00065+.5))*max(0,min(1,(.084-abs(x))/.001+.5))
        # Rounded eyebrow strokes painted on the same head map as the smile.
        local=abs(x)-.113
        clamped=max(-.048,min(.048,local))
        brow_z=1.224+.019*(1-(clamped/.052)**2)
        distance=math.sqrt((local-clamped)**2+(z-brow_z)**2)
        coverage=max(coverage,max(0,min(1,(.008+.006*(1-(clamped/.048)**2)-distance)/.0008+.5)))
        color=[.47,.76,.17]
        # Eyes, pupils and catchlights share the face texture; no eye shells.
        eye_x=abs(x)-.108
        for dx,dz,rx,rz,paint in [
            (eye_x,z-1.125,.068,.068,(.025,.032,.04)),
            (eye_x,z-1.125,.066,.066,(.96,.89,.72)),
            (eye_x,z-1.125,.039,.041,(.025,.032,.04)),
            (min(abs(x-.096),abs(x+.120)),z-1.146,.011,.012,(1,1,.98)),
        ]:
            distance=math.sqrt((dx/rx)**2+(dz/rz)**2)
            alpha=max(0,min(1,(1-distance)/.025+.5))
            color=[a*(1-alpha)+b*alpha for a,b in zip(color,paint)]
        color=[a*(1-coverage)+b*coverage for a,b in zip(color,(.025,.032,.04))]
        pixels.extend((*color,1))
image.pixels.foreach_set(pixels)
image.filepath_raw=str(TEXTURE_DIR/'face_smile_basecolor.png')
image.file_format='PNG'
image.save()
image.pack()
for node in face_mat.node_tree.nodes:
    if node.type=='TEX_IMAGE': node.image=image
HEAD.data.materials.append(face_mat)
uv=HEAD.data.uv_layers.active
for poly in HEAD.data.polygons:
    if poly.normal.y < -.35:
        poly.material_index=1
        for li in poly.loop_indices:
            co=HEAD.data.vertices[HEAD.data.loops[li].vertex_index].co
            uv.data[li].uv=(co.x/.574+.5,co.z/.63+.5)
# An outlined pinna and soft lobe; internal folds are color only.
ear_image=bpy.data.images.new('Ear_Folds_BaseColor',width=512,height=512,alpha=True)
ear_pixels=[]
for row in range(512):
    v=(row+.5)/512
    for col in range(512):
        u=(col+.5)/512
        # Upper helix, concha and short antihelix: directional painted depth.
        dx=(u-.54)/.32; dz=(v-.58)/.32
        d=math.sqrt(dx*dx+dz*dz)
        upper=max(0,min(1,(v-.38)/.12))
        rim=math.exp(-((d-.91)/.085)**2)*upper
        hollow=math.exp(-(((u-.46)/.18)**2+((v-.48)/.21)**2)*1.7)
        inner=math.exp(-((math.sqrt(((u-.43)/.17)**2+((v-.42)/.18)**2)-.95)/.12)**2)
        inner*=max(0,min(1,(u-.39)/.10))*max(0,min(1,(.60-v)/.12))
        light=math.exp(-((d-1.10)/.08)**2)*upper
        alpha=.65*rim+.85*hollow+.45*inner
        ear_pixels.extend((.47-.30*alpha+.025*light,.76-.39*alpha+.025*light,.17-.11*alpha+.008*light,1))
ear_image.pixels.foreach_set(ear_pixels)
ear_image.filepath_raw=str(TEXTURE_DIR/'ear_folds_basecolor.png');ear_image.file_format='PNG';ear_image.save();ear_image.pack()
EAR_MAT=SKIN.copy();EAR_MAT.name='Ear_Painted_Folds'
for node in EAR_MAT.node_tree.nodes:
    if node.type=='TEX_IMAGE':node.image=ear_image

def create_ear(side,sign):
    return bind_later(build_ear(side,sign,CHAR,EAR_MAT),'head')

for side,sign in [('L',1),('R',-1)]:
    create_ear(side,sign)
    bind_later(cylinder_between('Bolt_Washer.'+side,(sign*.28,0,1.275),(sign*.303,0,1.275),.055,CHARCOAL,24),'head')
    bind_later(cylinder_between('Bolt_Stem.'+side,(sign*.303,0,1.275),(sign*.362,0,1.275),.026,METAL,24),'head')
    bind_later(cylinder_between('Bolt_Rim.'+side,(sign*.383,0,1.275),(sign*.386,0,1.275),.056,METAL,32),'head')
    bind_later(cylinder_between('Bolt_Cap.'+side,(sign*.362,0,1.275),(sign*.394,0,1.275),.052,METAL,32),'head')
NOSE=bind_later(sphere('Nose',(0,-.222,1.049),(.043,.032,.039),SKIN),'head')
for vertex in NOSE.data.vertices:
    vertex.co.x*=1-.38*(vertex.co.z/.039)
    if vertex.co.y < -.025: vertex.co.y=-.025+(vertex.co.y+.025)*.55

def create_rig() -> bpy.types.Object:
    armature_data = bpy.data.armatures.new("Frank_Skeleton")
    rig = bpy.data.objects.new("Frank_Rig", armature_data)
    CHAR.objects.link(rig)
    rig.show_in_front = True
    rig.data.display_type = "STICK"
    bpy.context.view_layer.objects.active = rig
    rig.select_set(True)
    bpy.ops.object.mode_set(mode="EDIT")

    bones: dict[str, bpy.types.EditBone] = {}

    def bone(name: str, head, tail, parent: str | None = None) -> None:
        item = armature_data.edit_bones.new(name)
        item.head = head
        item.tail = tail
        item.use_deform = True
        if parent:
            item.parent = bones[parent]
        bones[name] = item

    bone("root", (0, 0, 0), (0, 0, 0.09))
    bone("pelvis", (0, 0, 0.34), (0, 0, 0.52), "root")
    bone("spine", (0, 0, 0.52), (0, 0, 0.86), "pelvis")
    bone("neck", (0, 0, 0.86), (0, 0, 1.02), "spine")
    bone("head", (0, 0, 1.02), (0, 0, 1.46), "neck")
    # Rigify and Blender use +X for the character's left side when facing -Y.
    for side, sign in (("L", 1), ("R", -1)):
        shoulder = f"shoulder.{side}"
        upper = f"upper_arm.{side}"
        forearm = f"forearm.{side}"
        hand = f"hand.{side}"
        fingers = f"fingers.{side}"
        bone(shoulder, (0, 0, 0.86), (0.156 * sign, 0, 0.765), "spine")
        bone(upper, (0.156 * sign, 0, 0.765), (0.274 * sign, 0, 0.620), shoulder)
        bone(forearm, (0.274 * sign, 0, 0.620), (0.345 * sign, 0, 0.480), upper)
        bone(hand, (0.345 * sign, 0, 0.480), (0.377 * sign, 0, 0.387), forearm)
        bone(fingers, (0.377 * sign, 0, 0.387), (0.405 * sign, 0, 0.320), hand)
        # Local X is the axis across the palm; +X flexion curls inward.
        bones[fingers].roll = -sign * math.pi / 2
        thigh = f"thigh.{side}"
        shin = f"shin.{side}"
        foot = f"foot.{side}"
        toes = f"toes.{side}"
        bone(thigh, (0.10 * sign, 0, 0.425), (0.13 * sign, 0, 0.23), "pelvis")
        bone(shin, (0.13 * sign, 0, 0.23), (0.13 * sign, 0, 0.09), thigh)
        bone(foot, (0.13 * sign, 0, 0.09), (0.13 * sign, -0.08, 0.05), shin)
        bone(toes, (0.13 * sign, -0.08, 0.05), (0.13 * sign, -0.145, 0.04), foot)
    bpy.ops.object.mode_set(mode="OBJECT")
    rig.select_set(False)
    return rig


RIG = create_rig()

# Subdivision interpolates anatomical control-ring weights. Trim only negligible
# tails and retain at most four influences, without spatial candidate thresholds.
for vertex in BODY.data.vertices:
    memberships=sorted(((g.group,g.weight) for g in vertex.groups),key=lambda item:item[1],reverse=True)[:4]
    total=sum(w for _,w in memberships)
    if total <= 0: raise RuntimeError("Unweighted control cage vertex")
    for group in BODY.vertex_groups:group.remove([vertex.index])
    for index,weight in memberships:BODY.vertex_groups[index].add([vertex.index],weight/total,'REPLACE')

body_modifier = BODY.modifiers.new("Frank_Skin", "ARMATURE")
body_modifier.object = RIG
body_modifier.use_vertex_groups = True
BODY.parent = RIG

for bone_name, objects in parts_by_bone.items():
    for obj in objects:
        group = obj.vertex_groups.new(name=bone_name)
        group.add(range(len(obj.data.vertices)), 1.0, "REPLACE")
        modifier = obj.modifiers.new("Frank_Skin", "ARMATURE")
        modifier.object = RIG
        modifier.use_vertex_groups = True
        obj.parent = RIG

# Metadata used by downstream inspection without relying on Blender UI state.
RIG["frank_asset_version"] = "1.4"
RIG["height_m"] = 1.5
RIG["rest_pose"] = "A-pose"
RIG["front_axis"] = "-Y"
RIG["up_axis"] = "+Z"
RIG["max_weights_per_vertex"] = 4
RIG["source_provenance"] = "Procedural original base; male_apose_gpt turnaround"
RIG["source_security"] = "Procedural geometry; no source datablocks imported"


def add_qa_environment() -> tuple[bpy.types.Object, list[bpy.types.Object]]:
    world = bpy.data.worlds.new("QA_World") if not bpy.data.worlds else bpy.data.worlds[0]
    bpy.context.scene.world = world
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.32, 0.32, 0.32, 1)
    world.node_tree.nodes["Background"].inputs["Strength"].default_value = 0.35

    camera_data = bpy.data.cameras.new("QA_Camera")
    camera = bpy.data.objects.new("QA_Camera", camera_data)
    QA.objects.link(camera)
    camera_data.type = "ORTHO"
    camera_data.ortho_scale = 1.72
    bpy.context.scene.camera = camera

    lights: list[bpy.types.Object] = []
    for name, location, energy, color, size in (
        ("QA_Key", (-3.0, -4.0, 4.0), 650.0, (1.0, 1.0, 1.0), 4.0),
        ("QA_Fill", (3.0, -2.0, 2.4), 420.0, (1.0, 1.0, 1.0), 3.0),
        ("QA_Rim", (0.0, 3.0, 3.2), 520.0, (1.0, 1.0, 1.0), 3.0),
    ):
        data = bpy.data.lights.new(name, "AREA")
        data.energy = energy
        data.color = color
        data.shape = "DISK"
        data.size = size
        light = bpy.data.objects.new(name, data)
        light.location = location
        light.rotation_euler = (Vector((0,0,.8))-light.location).to_track_quat("-Z","Y").to_euler()
        QA.objects.link(light)
        lights.append(light)
    return camera, lights


CAMERA, LIGHTS = add_qa_environment()
scene = bpy.context.scene
scene.render.engine = "CYCLES"
scene.cycles.samples = 48
scene.cycles.use_denoising = True
scene.render.resolution_x = 720
scene.render.resolution_y = 720
scene.render.resolution_percentage = 100
scene.render.image_settings.file_format = "PNG"
scene.render.film_transparent = False
scene.render.image_settings.color_mode = "RGBA"
try:
    scene.view_settings.look = "AgX - Medium High Contrast"
except (TypeError, ValueError):
    pass


def point_camera(location, target=(0, 0, 0.78), ortho_scale=1.72) -> None:
    CAMERA.location = location
    CAMERA.rotation_euler = (Vector(target) - CAMERA.location).to_track_quat("-Z", "Y").to_euler()
    CAMERA.data.ortho_scale = ortho_scale


def render(name: str, location, target=(0, 0, 0.78), ortho_scale=1.72) -> None:
    point_camera(location, target, ortho_scale)
    scene.render.filepath = str(QA_DIR / f"{name}.png")
    bpy.ops.render.render(write_still=True)


views = {
    "view_front": ((0, -4.5, 0.77), (0, 0, 0.77), 1.68),
    "view_back": ((0, 4.5, 0.77), (0, 0, 0.77), 1.68),
    "view_side": ((4.5, 0, 0.77), (0, 0, 0.77), 1.68),
    "view_three_quarter_left": ((-3.2, -3.2, 1.25), (0, 0, 0.80), 1.72),
    "view_three_quarter_right": ((3.2, -3.2, 1.25), (0, 0, 0.80), 1.72),
    "closeup_face": ((0, -4.0, 1.27), (0, 0, 1.25), 0.72),
    "closeup_ear": ((2,-3,1.17),(.315,0,1.105),.24),
    "closeup_foot": ((1,-3,.5),(.13,-.045,.065),.30),
    "closeup_pelvis": ((0,-3,.45),(0,0,.44),.50),
    "closeup_hand": ((3, 0, 1), (0.375, 0, 0.405), 0.27),
}
if not globals().get("FRANK_PREVIEW_ONLY", False):
    for view_name, (position, target, scale) in views.items():
        render(view_name, position, target, scale)


def reset_pose() -> None:
    for pose_bone in RIG.pose.bones:
        pose_bone.location = (0, 0, 0)
        pose_bone.rotation_mode = "XYZ"
        pose_bone.rotation_euler = (0, 0, 0)
        pose_bone.scale = (1, 1, 1)
    bpy.context.view_layer.update()


def pose_and_render(
    name: str,
    rotations: dict[str, tuple[float, float, float]],
    camera=(0, -4.5, 1.05),
) -> None:
    reset_pose()
    for bone_name, rotation in rotations.items():
        RIG.pose.bones[bone_name].rotation_euler = rotation
    bpy.context.view_layer.update()
    if not globals().get("FRANK_PREVIEW_ONLY", False):
        render(name, camera, (0, 0, 0.76), 1.72)


pose_and_render(
    "pose_elbows_knees_90",
    {
        "forearm.L": (math.radians(90), 0, 0),
        "forearm.R": (math.radians(90), 0, 0),
        "shin.L": (math.radians(-90), 0, 0),
        "shin.R": (math.radians(-90), 0, 0),
    },
    (3.2, -3.2, 1.15),
)
pose_and_render(
    "pose_shoulders_head",
    {
        "upper_arm.L": (0, math.radians(-25), math.radians(-55)),
        "upper_arm.R": (0, math.radians(25), math.radians(55)),
        "head": (0, 0, math.radians(28)),
    },
)
pose_and_render(
    "pose_grip",
    {
        "hand.L": (0, math.radians(-20), math.radians(-15)),
        "fingers.L": (math.radians(55), 0, 0),
        "hand.R": (0, math.radians(20), math.radians(15)),
        "fingers.R": (math.radians(55), 0, 0),
    },
)
pose_and_render(
    "pose_sit",
    {
        "thigh.L": (math.radians(-82), 0, 0),
        "thigh.R": (math.radians(-82), 0, 0),
        "shin.L": (math.radians(92), 0, 0),
        "shin.R": (math.radians(92), 0, 0),
        "spine": (math.radians(8), 0, 0),
    },
    (3.2, -3.2, 1.15),
)
reset_pose()

# Strip executable and animation state before producing the editable master.
for datablock in list(bpy.data.texts):
    bpy.data.texts.remove(datablock)
for action in list(bpy.data.actions):
    bpy.data.actions.remove(action)
for obj in bpy.data.objects:
    if obj.animation_data:
        obj.animation_data_clear()
    data = getattr(obj, "data", None)
    if data is not None and getattr(data, "animation_data", None):
        data.animation_data_clear()

# Drop material/image dependencies pulled transitively with source body_low after
# its slots were replaced. Only Frank's generated, packed textures remain.
for material in list(bpy.data.materials):
    if material.users == 0:
        bpy.data.materials.remove(material)
for image in list(bpy.data.images):
    if image.users == 0:
        bpy.data.images.remove(image)

if bpy.data.texts or bpy.data.actions:
    raise RuntimeError("Executable or animation datablocks survived sanitization")
for obj in CHAR.all_objects:
    if obj.animation_data and obj.animation_data.drivers:
        raise RuntimeError(f"Driver survived sanitization on {obj.name}")

for obj in CHAR.all_objects:
    if obj.type == "MESH":
        obj.data.calc_loop_triangles()
mesh_triangles = sum(len(obj.data.loop_triangles) for obj in CHAR.all_objects if obj.type == "MESH")
RIG["triangle_count"] = mesh_triangles
if mesh_triangles > 30000:
    raise RuntimeError(f"Triangle budget exceeded: {mesh_triangles}")

point_camera((0,-4.5,.77),(0,0,.77),1.68)
scene.frame_start = 1
scene.frame_end = 1
scene.frame_set(1)
bpy.ops.wm.save_as_mainfile(filepath=str(BLEND_PATH), check_existing=False, compress=True)

# Export only the character collection; QA cameras/lights never enter the GLB.
bpy.ops.object.select_all(action="DESELECT")
for obj in CHAR.all_objects:
    obj.select_set(True)
bpy.context.view_layer.objects.active = RIG
bpy.ops.export_scene.gltf(
    filepath=str(GLB_PATH),
    export_format="GLB",
    use_selection=True,
    export_animations=False,
    export_skins=True,
    export_morph=False,
    export_lights=False,
    export_cameras=False,
    export_yup=True,
)
bpy.ops.wm.save_as_mainfile(filepath=str(BLEND_PATH), check_existing=False, compress=True)
print(f"FRANK_BUILD_COMPLETE blend={BLEND_PATH} glb={GLB_PATH}")
