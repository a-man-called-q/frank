"""Uniform-scale QA contact sheets. Never warp a reference to fit the model.

Run with a Python containing Pillow. --bundle selects a staged output directory.
"""
from pathlib import Path
import argparse
import json
from PIL import Image,ImageDraw,ImageFont

ROOT=Path(__file__).resolve().parents[4]
parser=argparse.ArgumentParser();parser.add_argument('--bundle',type=Path,required=True);parser.add_argument('--baseline-version',default='1.3');parser.add_argument('--version',default='1.4');args=parser.parse_args()
qa=args.bundle/'qa';reference=ROOT/'assets/character_model/frank/references/male_apose_gpt'
baseline=ROOT/'assets/character_model/frank/backups'/args.baseline_version/'qa'
font=ImageFont.truetype('/System/Library/Fonts/Helvetica.ttc',20)
small=ImageFont.truetype('/System/Library/Fonts/Helvetica.ttc',15)

def panel(path,label,reference_image=False):
    src=Image.open(path).convert('RGB')
    if reference_image:
        factor=600/(1188 if path.stem=='right' else 1148);floor_source=1265 if path.stem=='right' else 1248
    else:
        factor=600/(src.height*1.5/1.68)
        floor_source=src.height*(.5+.77/1.68)
    resized=src.resize((round(src.width*factor),round(src.height*factor)),Image.Resampling.LANCZOS)
    result=Image.new('RGB',(530,720),(238,238,238))
    result.paste(resized,((530-resized.width)//2,round(655-floor_source*factor)))
    d=ImageDraw.Draw(result);d.rectangle((0,0,530,42),fill=(238,238,238));d.text((15,11),label,font=font,fill=(25,25,25))
    d.line((0,655,530,655),fill=(208,99,57),width=2)
    return result

for name,ref in [('front','front'),('back','back'),('side','right')]:
    images=[panel(reference/(ref+'.png'),'Reference',True),panel(baseline/('view_'+name+'.png'),'Frank '+args.baseline_version),panel(qa/('view_'+name+'.png'),'Frank '+args.version)]
    canvas=Image.new('RGB',(1590,720),(238,238,238))
    for i,im in enumerate(images):canvas.paste(im,(530*i,0))
    ImageDraw.Draw(canvas).text((15,685),'Same height and floor; uniform scaling only. Side reference is slightly turned.',font=small,fill=(35,35,35))
    canvas.save(qa/('comparison_'+name+'.png'))
# Detail comparisons retain each camera crop: for visual diagnosis, not measurement.
for name in ('hand','foot','ear','pelvis','face'):
    files=[baseline/('closeup_'+name+'.png'),qa/('closeup_'+name+'.png')]
    result=Image.new('RGB',(1000,545),(238,238,238));d=ImageDraw.Draw(result)
    for i,(path,label) in enumerate(zip(files,('Frank '+args.baseline_version,'Frank '+args.version))):
        im=Image.open(path).convert('RGB');im.thumbnail((500,500),Image.Resampling.LANCZOS)
        result.paste(im,(i*500+(500-im.width)//2,45+(500-im.height)//2));d.text((i*500+15,12),label+' — '+name,font=font,fill=(25,25,25))
    result.save(qa/('comparison_detail_'+name+'.png'))
# Unaltered source crops make the detailed target explicit.
front=Image.open(reference/'front.png')
for name,box in {'head':(270,90,910,590),'hand':(250,820,375,1010),'feet':(390,1120,780,1280),'pelvis':(430,820,745,995)}.items():
    front.crop(box).save(qa/('reference_detail_'+name+'.png'))
# Compact review board, refreshed from the current bundle (including the grip).
names=['clay_view_front','clay_view_back','clay_view_side','closeup_ear','closeup_foot','closeup_hand',
       'closeup_hand_palm','clay_closeup_pelvis','closeup_neck','closeup_neck_back','pose_grip_close','pose_sit']
board=Image.new('RGB',(1440,1170),(238,238,238));draw=ImageDraw.Draw(board)
for i,name in enumerate(names):
    im=Image.open(qa/(name+'.png')).convert('RGB').resize((360,360),Image.Resampling.LANCZOS)
    x=i%4*360;y=i//4*390;board.paste(im,(x,y+30));draw.text((x+8,y+7),name,font=small,fill=(25,25,25))
board.save(qa/'review_contact_sheet.png')
# A separately labelled approximation to the source side camera; no image warp.
matched=Image.new('RGB',(1060,720),(238,238,238))
matched.paste(panel(reference/'right.png','Reference (slightly turned)',True),(0,0))
matched.paste(panel(qa/'view_reference_side.png','Frank '+args.version+' / estimated yaw 7.8 deg'),(530,0))
ImageDraw.Draw(matched).text((15,685),'Uniform height/floor; inferred camera, approximate depth. See true side comparison separately.',font=small,fill=(35,35,35))
matched.save(qa/'comparison_side_matched.png')
# Detail crops are qualitative: fit inside equal panels without stretching.
head_board=Image.new('RGB',(1500,550),(238,238,238));draw=ImageDraw.Draw(head_board)
for i,(path,label) in enumerate([(qa/'reference_detail_head.png','Reference'),(baseline/'closeup_face.png','Frank '+args.baseline_version),(qa/'closeup_face.png','Frank '+args.version)]):
    im=Image.open(path).convert('RGB');im.thumbnail((500,500),Image.Resampling.LANCZOS)
    head_board.paste(im,(i*500+(500-im.width)//2,45+(500-im.height)//2));draw.text((i*500+15,12),label,font=font,fill=(25,25,25))
head_board.save(qa/'comparison_head.png')
