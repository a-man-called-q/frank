# Frank 1.4 — tinjauan visual dan teknis

Kepala, telinga, dan volume torso direvisi terhadap referensi. Hasil visual ditinjau terpisah dari ukuran dan tes pemuatan.

Master dan GLB impor ulang: **29,388 triangle**, 23 tulang, maksimal tiga pengaruh aktif, nol vertex tanpa bobot. Validasi teknis dan Moon character-smoke lolos.

| Bagian | Hasil | Bukti |
|---|---|---|
| head | Crown and jaw use independent sections, cheeks curve into sides; five traced head widths differ by less than 5%. | [comparison_head.png](comparison_head.png) |
| ear | Embedded pinna has a medial indentation, rounded lobe and variable thickness; painted rim/concha/short fold remain readable. Internal detail remains a texture approximation. | [comparison_detail_ear.png](comparison_detail_ear.png) |
| face | Brow tips taper without the former central cusp; nose has a wider lower base and blunt rounded triangle profile. Eyes and expression remain painted. | [comparison_head.png](comparison_head.png) |
| side_volume | Anterior belly and posterior seat are fuller; torso side contour no longer reads as a thin straight slab. Front widths remain within the baseline tolerances. | [comparison_side_matched.png](comparison_side_matched.png) |
| junctions_uv | Clay pelvis and shoulder transitions are continuous; no black texture spill onto hands or legs observed in the rendered views. | [review_contact_sheet.png](review_contact_sheet.png) |
| poses | The early sitting spikes were eliminated by preserving weighted joint loops. Reviewed elbow/knee, shoulder, grip and sitting renders; no runaway vertices observed. Soft FK compression at the inner knee and hip remains. | [pose_sit.png](pose_sit.png) |
| roundtrip_runtime | Imported GLB renders retain the silhouette, materials and rig; macOS character-smoke passed for the same GLB hash. | [glb_three_quarter.png](glb_three_quarter.png) |

## Batas hasil
- Side-camera yaw 7.8 degrees and depth are estimates; reference views are not a calibrated orthographic set.
- Painted ear/face details remain static approximations, with no geometric internal ear grooves or facial rig.
- The simple FK rig retains compression creases at tightly bent knees/hips; no corrective shapes were introduced.
- Additional head/ear/side calibration was recorded after initial construction preview; the chronology is stated in reference-calibration-1.4.json.

## Lokasi regangan terbesar

| Pose | Edge | Rasio | Titik tengah saat rest (m) |
|---|---:|---:|---|
| elbows_knees_90 | 22194 | 1.885× | -0.2354, -0.0468, 0.6542 |
| shoulders | 21491 | 2.338× | -0.1681, 0.0498, 0.7395 |
| grip | 23956 | 2.326× | -0.4047, -0.0418, 0.3280 |
| sit | 9775 | 2.199× | 0.1200, 0.1010, 0.4260 |

## Hash aset

- Blend: `7c1d01c045d5431fcb472bc54af2be6a60ca4b13fb2a33b2d934c284c642e372`
- GLB: `c0fffeee515dcb2172fe525a1d5935cca81a883edf81499adf1857614789964e`
