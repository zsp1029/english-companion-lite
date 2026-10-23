from pathlib import Path
import struct, math
# Supersampled rendering of the code-native speech mark; SVG remains the editable source.
Path('assets/companion.svg').write_text('''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><path fill="#2563eb" d="M9 3h14a6 6 0 0 1 6 6v12a6 6 0 0 1-6 6h-8l-7 4v-4a6 6 0 0 1-5-6V9a6 6 0 0 1 6-6Z"/><path fill="none" stroke="#fff" stroke-width="2.8" stroke-linecap="round" stroke-linejoin="round" d="M21 10H12v11h9M12 15.5h7"/></svg>''',encoding='utf-8')
def inside(x,y):
    body=(3<=x<=29 and 9<=y<=21) or (9<=x<=23 and 3<=y<=27) or any((x-cx)**2+(y-cy)**2<=36 for cx,cy in [(9,9),(23,9),(9,21),(23,21)])
    return body or (8<=x<=15 and 25<=y<=31-(x-8)*4/7)
def line(x,y,a,b):
    dx,dy=b[0]-a[0],b[1]-a[1]; t=max(0,min(1,((x-a[0])*dx+(y-a[1])*dy)/(dx*dx+dy*dy)))
    return (x-a[0]-t*dx)**2+(y-a[1]-t*dy)**2<=1.4**2
entries=[]; blobs=[]; offset=6+16*3
for n in (16,32,64):
    pixels=bytearray()
    for py in range(n-1,-1,-1):
        for px in range(n):
            sums=[0,0,0]; count=0
            for sy in range(4):
                for sx in range(4):
                    x=(px+(sx+.5)/4)*32/n;y=(py+(sy+.5)/4)*32/n
                    if inside(x,y):
                        c=(255,255,255) if any(line(x,y,a,b) for a,b in [((21,10),(12,10)),((12,10),(12,21)),((12,21),(21,21)),((12,15.5),(19,15.5))]) else (37,99,235)
                        for i in range(3):sums[i]+=c[i]
                        count+=1
            rgb=[round(v/count) if count else 0 for v in sums]
            pixels+=bytes([rgb[2],rgb[1],rgb[0],round(count*255/16)])
    blob=struct.pack('<IiiHHIIiiII',40,n,n*2,1,32,0,len(pixels),0,0,0,0)+pixels+bytes(((n+31)//32)*4*n)
    entries.append(struct.pack('<BBBBHHII',n,n,0,0,1,32,len(blob),offset)); blobs.append(blob);offset+=len(blob)
Path('assets/companion.ico').write_bytes(struct.pack('<HHH',0,1,3)+b''.join(entries)+b''.join(blobs))
