# Frank 3D base — shape revision 1.4

Editable master: `frank.blend`. Application asset:
`apps/frank_desktop/assets/character_model/frank.glb`.

## Shape and construction

The bald male turnaround in `references/male_apose_gpt/`
is the design reference. Front proportions take precedence; the slightly turned
side image supplies approximate depth. No third-party mesh or executable Blender
source is imported.

The body is now an authored connected control cage. Torso sockets, the pelvic
saddle, wrists, palm webs, thumb and fingers share boundary vertices. Two levels
of Catmull–Clark subdivision form the final skin. Limited planar simplification
is restricted to patches with one identical bone influence; weighted joint loops
are preserved, and resulting n-gons are triangulated in the rest pose. There is
no voxel union or boolean assembly in the skin builder. Anatomical ring
weights are interpolated through subdivision, normalized and limited to four
influences. The source cage is reproducible in `scripts/body_topology.py`.

- Each hand has three fingers and a thumb, with a flattened palm and open webs.
- Feet have a single rounded forefoot and flat contact area, with **no separate
  toe lobes**. The toes bones still deform the forefoot.
- The pelvis branches into the thighs; the center does not hang below its sides.
- Torso sections have independent front/back depths, providing a rounded belly
  and seat while retaining the calibrated front width.
- The neck is short and shoulders blend into the chest. Head and ear control
  cages live in `scripts/head_topology.py`; the head has a curved crown, cheek
  volume and a tapered jaw. Subdivision followed by reduction is restricted to
  these rigid head-bound parts. Brow tips taper and the nose has a blunt triangular profile.
- Ear silhouettes include a fuller lower lobe and embedded attachment. Their
  rim, concha and short inner fold are color textures, without groove geometry or normal maps.
- Eyes, pupils, highlights, brows, smile and briefs remain painted features.
  Body/face maps are 1024 square, and the ear map is 512 square.
- Torso/leg and arm/hand faces receive explicit material/UV regions. Brief color
  no longer depends on an X-coordinate switch in the arm UVs.
- Skin and briefs are matte. Bolt end faces have planar normals.

## Rig and asset contract

One `Frank_Rig`, 23 existing bone names and the same hierarchy, A-pose, about
1.5 m high, Blender +Z up / -Y front, and glTF +Y up. Finger bone roll is aligned
so local +X flexion curls toward the palm. Fingers remain grouped, without
independent articulation, IK, facial controls or animation clips.

The master and imported runtime asset contain **29,388 triangles**. The body is
one closed component with consistent winding and no degenerate faces. All
vertices have normalized weights; current maximum is three influences. Six
images are embedded in the GLB. There are no exported cameras, lights or clips.
Blender's importer creates a hidden bone-display icosphere; the validator
identifies it by its `custom_shape` references and excludes it from asset counts.

## Rebuild, inspect, and promote

Use Blender 5.2 with auto-run disabled, in a disposable process or saved scene.
The builder resets its scene. Its default output is the ignored `candidate-1.4/`
directory, including a candidate `frank.glb`; it never writes the application GLB.
Set `FRANK_OUTPUT_DIR` to select a different bundle directory. Set
`FRANK_PREVIEW_ONLY=True` in the script namespace to skip the builder's initial
renders and use the dedicated QA renderer instead.

For a build without redundant initial renders, run from the repository root:

```sh
/Applications/Blender.app/Contents/MacOS/Blender --background --factory-startup --disable-autoexec --python-exit-code 1 --python-expr 'import runpy; runpy.run_path("assets/character_model/frank/scripts/build_frank.py", init_globals={"FRANK_PREVIEW_ONLY": True})'
```

Use `--python-exit-code 1` for subsequent Blender checks too, so a Python failure
cannot be mistaken for a successful process exit.

Run the scripts in this order:

1. `build_frank.py` — build the candidate master, packed textures and GLB.
2. Open the saved candidate in a disposable Blender process and run
   `inspect_geometry.py`, then `render_qa.py`. This renders six views, close-ups,
   clay surfaces, silhouettes and four poses, with localized edge-stretch data.
3. Run `validate_frank.py` in a disposable process. It imports the GLB into an
   empty scene and writes technical results; **it replaces the process's scene**.
4. Run `render_roundtrip.py` on the candidate master to render the imported GLB.
5. Run `check_runtime.py` with ordinary Python. It temporarily stages the GLB,
   runs `moon run frank-desktop:character-smoke`, captures the actual macOS frame,
   and restores the previous runtime asset in a `finally` block.
6. Run `compare_renders.py --bundle assets/character_model/frank/candidate-1.4`
   from the repository root using Python with Pillow. Inspect its comparisons
   and all detailed/pose renders; record the actual review in
   `candidate-1.4/qa/visual-review.json`.
7. `promote_candidate.py` refuses stale hashes, failed technical/reference/runtime
   checks, unresolved visual findings or edits to the original assets since the
   baseline backup. It copies the verified candidate to the master and app paths,
   preserves previous QA/textures, and verifies the resulting hashes.

The scripts resolve the repository root from their own location (or an explicit
`--repo-root` argument), so they remain portable across checkouts. `compare_renders.py` additionally
uses the macOS Helvetica font. Candidate and backup directories are ignored;
the promoted master, builder, textures and current QA are the deliverables.

## Evidence and limits

`qa/validation.json`, `geometry-validation.json`, `pose-validation.json`,
`render-manifest.json`, `roundtrip-render.json`, `runtime-validation.json` and
`visual-review.json` identify the exact assets they examined. `promotion.json`
records the master/runtime readback. A technical pass alone is never a likeness
approval, and the Flutter smoke test only establishes loading and visible pixels.

The six calibrated widths, four torso contour samples, five vertical landmarks
and five additional head contour samples are within 5% of their stated targets. These are sampled dimensions,
not a claim that every pixel or all side-view anatomy matches. Initial rough
contour bounds and corrected source-pixel traces are both preserved in
`reference-calibration-1.4.json`, with the measurement method. The reproducible source is `reference-calibration.json`;
the builder copies it into each candidate QA bundle. Additional 1.4 calibration
was recorded after the first construction preview; the report preserves that
chronology. The matched side comparison uses an explicitly estimated 7.8-degree
yaw, and the true side render is also supplied.

The FK rig can produce soft compression folds at deeply bent knees and hips;
there are no corrective shapes. The grouped grip is a simple coordinated curl,
not a fully articulated fist. Ear folds and facial shading remain static painted
approximations of the reference. See the visual review for assessed evidence.

The complete prior baseline is retained locally at `backups/1.3/`, with hashes.
Previous QA and texture folders are also retained in the promotion archive.
