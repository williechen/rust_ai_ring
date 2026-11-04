const { chromium } = require('playwright');
const fs=require('node:fs');
const path=require('node:path');
const root=process.argv[2] || path.resolve(__dirname,'../../../..');
(async()=>{
 const b=await chromium.launch({headless:true, executablePath:process.env.PET_CHROMIUM_EXECUTABLE || undefined});try { const page=await b.newPage({viewport:{width:200,height:200}});
 const source=fs.readFileSync(path.join(root,'pet_apps/pet_ui/src/component/frames.rs'),'utf8');
 const frames=Object.fromEntries([...source.matchAll(/const (\w+): &str =\s*"(data:image[^"]+)";/g)].map(m=>[m[1],m[2]]));
 const src=frames.IDLE;
 if(Object.keys(frames).length!==6)throw Error("缺少六個完整圖幀");
 const css=fs.readFileSync(path.join(root,'pet_apps/pet_ui/style.css'),'utf8');
 await page.setContent(`<style>${css}</style><main class="pet-life"><header class="drag-handle">...</header><p>測試寵物</p><section class="pet-status"><div class="pet-face"><img class="pose-idle" src="${src}" alt="寵物動畫"></div><strong>待機</strong><small>revision 1</small></section><button>回籠子</button><p role="alert"></p><button>控制台</button></main>`);
 for(const pose of ['idle','blink','interact-a','interact-b','sleep-a','sleep-b']){
  const src=frames[pose.replaceAll('-','_').toUpperCase()];
  await page.locator('img').evaluate((e,p)=>{e.className='pose-'+p.pose;e.src=p.src;},{pose,src});
  await page.locator('img').evaluate(e=>e.decode());
  const result=await page.evaluate(()=>{const f=document.querySelector('.pet-face').getBoundingClientRect();const i=document.querySelector('img').getBoundingClientRect();return {frame:{width:f.width,height:f.height},image:{x:i.x,y:i.y,width:i.width,height:i.height},count:document.querySelectorAll('.pet-face img').length,scroll:document.documentElement.scrollHeight}});
  console.log(pose,JSON.stringify(result));
  if(result.frame.width!==200||result.frame.height!==200||result.count!==1)throw Error('200x200 single-image canvas missing');
  if(result.image.x<0||result.image.y<0||result.image.x+result.image.width>200.5||result.image.y+result.image.height>200.5)throw Error('pose clipped');
 }
 await page.screenshot({path:process.env.PET_SCREENSHOT || 'pet-day45-canvas.png'});} finally { await b.close(); }
})().catch(e=>{console.error(e);process.exitCode=1});