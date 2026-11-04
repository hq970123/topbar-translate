#!/usr/bin/env python3
"""Store poster art (9:16, 1440x2160) and box art (1:1, 2160x2160), zh-CN + en-US."""
from PIL import Image, ImageDraw, ImageFont, ImageFilter
import os
root=os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
icon=Image.open(f"{root}/src-tauri/icons/icon.png").convert("RGBA")
shot=Image.open(f"{root}/store/screenshots/1-translate.png").convert("RGBA")
panel=shot.crop((560,0,2000,662))
B="/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc"; R="/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"
def grad(w,h):
    top=(108,150,226); bot=(184,158,230)
    g=Image.new("RGBA",(w,h))
    d=ImageDraw.Draw(g)
    for y in range(h):
        t=y/(h-1); d.line([(0,y),(w,y)],fill=tuple(int(a+(b-a)*t) for a,b in zip(top,bot))+(255,))
    return g
def ctext(d,y,s,f,w,fill):
    tw=d.textlength(s,font=f); d.text(((w-tw)/2,y),s,font=f,fill=fill)
def paste_shadow(c,im,x,y,r=40,a=90):
    sh=Image.new("RGBA",c.size,(0,0,0,0)); m=Image.new("L",im.size,0)
    ImageDraw.Draw(m).rounded_rectangle((0,0,*im.size),radius=r,fill=a)
    sh.paste((40,40,90,255),(x,y+24),m); sh=sh.filter(ImageFilter.GaussianBlur(36))
    c.alpha_composite(sh)
    mk=Image.new("L",im.size,0); ImageDraw.Draw(mk).rounded_rectangle((0,0,*im.size),radius=r,fill=255)
    c.paste(im,(x,y),mk)
T={"zh":("顶部悬浮翻译","鼠标移到屏幕顶部","翻译框自动滑下","截图翻译 · 离线语音 · 免费"),
   "en":("TopBar Translate","Touch the top of the screen,","the translator slides down","Screenshot · Offline voice · Free")}
for lang,(name,l1,l2,l3) in T.items():
    W,H=1440,2160; c=grad(W,H); d=ImageDraw.Draw(c)
    ic=icon.resize((360,360),Image.LANCZOS); c.alpha_composite(ic,((W-360)//2,250))
    fs=150 if lang=="zh" else 120
    ctext(d,660,name,ImageFont.truetype(B,fs),W,"white")
    f2=ImageFont.truetype(B,76 if lang=="zh" else 62)
    ctext(d,900,l1,f2,W,(255,255,255,240)); ctext(d,1000,l2,f2,W,(255,255,255,240))
    ctext(d,1140,l3,ImageFont.truetype(R,52 if lang=="zh" else 46),W,(255,255,255,215))
    pw=1280; p=panel.resize((pw,int(panel.height*pw/panel.width)),Image.LANCZOS)
    paste_shadow(c,p,(W-pw)//2,1380,r=34)
    c.convert("RGB").save(f"{root}/store/art/poster-{lang}.png")
    c.convert("RGB").resize((720,1080),Image.LANCZOS).save(f"{root}/store/art/poster-{lang}-720x1080.png")
    S=2160; b=grad(S,S); d=ImageDraw.Draw(b)
    ic=icon.resize((1000,1000),Image.LANCZOS); b.alpha_composite(ic,((S-1000)//2,380))
    ctext(d,1480,name,ImageFont.truetype(B,200 if lang=="zh" else 170),S,"white")
    b.convert("RGB").save(f"{root}/store/art/box-{lang}.png")
    b.convert("RGB").resize((1080,1080),Image.LANCZOS).save(f"{root}/store/art/box-{lang}-1080.png")
print("ok")
