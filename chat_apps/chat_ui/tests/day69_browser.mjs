import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {once} from 'node:events';
import {createRequire} from 'node:module';
import {mkdtempSync,mkdirSync,writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {DatabaseSync} from 'node:sqlite';
import {setTimeout as delay} from 'node:timers/promises';

const phase=process.env.PROBE_PHASE??'green',out=resolve(process.env.PROBE_OUTPUT_DIR??'outputs/day64-acceptance-2026-11-17/chat-r2');mkdirSync(out,{recursive:true});
const binary=process.env.CHAT_BINARY,site=process.env.CHAT_SITE_ROOT;assert(binary&&site,'請指定同來源的後端與前端');
const fixture=mkdtempSync(join(tmpdir(),'day64-accepted-chat-r2-')),dbpath=join(fixture,'chat.sqlite'),db=new DatabaseSync(dbpath);db.exec('PRAGMA journal_mode=WAL');
const base='http://127.0.0.1:3000',cases=[],errors=[],responses=[],contexts=[];let server,browser,serverLog='';
const {chromium}=createRequire(import.meta.url)('playwright');
function record(name){cases.push({name,status:'PASS'});console.log(JSON.stringify(cases.at(-1)));}
async function start(){
 assert(await fetch(base+'/health').then(()=>false,()=>true),'測試埠3000須空閒');
 const child=spawn(binary,[],{cwd:fixture,env:{...process.env,CHAT_DATABASE_URL:'sqlite://'+dbpath,LEPTOS_SITE_ROOT:site,LEPTOS_OUTPUT_NAME:'rust_ai_chat',LEPTOS_SITE_PKG_DIR:'pkg',LEPTOS_SITE_ADDR:'127.0.0.1:3000'},stdio:['ignore','pipe','pipe']});
 child.stdout.on('data',b=>serverLog+=b);child.stderr.on('data',b=>serverLog+=b);
 for(let i=0;i<100;i++){assert.equal(child.exitCode,null,serverLog);if(await fetch(base+'/health').then(r=>r.ok,()=>false))return child;await delay(100);}
 throw Error('伺服器未就緒');
}
async function stop(s){if(s&&s.exitCode===null&&s.signalCode===null){const done=once(s,'exit');s.kill('SIGTERM');await done;}}
async function context(){
 const ctx=await browser.newContext();contexts.push(ctx);ctx.on('page',p=>{
  p.on('pageerror',e=>errors.push({type:'pageerror',url:p.url(),text:String(e)}));
  p.on('console',m=>{if(m.type()==='error'&&/hydrat|unreachable/i.test(m.text()))errors.push({type:'console',url:p.url(),text:m.text()});});
  p.on('response',r=>{if(r.url().includes('/pkg/'))responses.push({url:r.url(),status:r.status()});});
 });return ctx;
}
async function wait(fn,label){
 for(let i=0;i<150;i++){assert.equal(errors.length,0,JSON.stringify(errors));if(await fn())return;await delay(100);}
 throw Error(label+'逾時');
}
async function navigation(p,label){await wait(async()=>await p.locator('nav[aria-label="主要導覽"] .site-session').innerText()===label&&await p.locator('.site-status').innerText()!=='確認登入狀態中','導覽狀態');}
async function signup(p,user){
 await p.goto(base+'/signup');for(const [name,value]of Object.entries({username:user,password:'Synthetic-hydration-password',display_name:({hydrate_a:'小安',hydrate_b:'小晴',hydrate_c:'小宇'}[user]??user),email:user+'@example.test'}))await p.locator(`[name=${name}]`).fill(value);
 await Promise.all([p.waitForURL(base+'/signup/complete'),p.locator('form[action="/signup"] button').click()]);
}
async function login(p,user,next=''){
 await p.goto(base+'/login'+(next?'?next='+encodeURIComponent(next):''));assert.equal(new URL(p.url()).pathname,'/signin');
 await navigation(p,'登入');await p.locator('[name=username]').fill(user);await p.locator('[name=password]').fill('Synthetic-hydration-password');
 await Promise.all([p.waitForURL(base+(next||'/account')),p.locator('form[action="/login"] button').click()]);
}
try{
 server=await start();
 const demoNames=['程式設計交流','生活閒聊','新手問答','音樂分享','閱讀交流','寵物天地','旅遊散步','遊戲同好','語言學習','電影討論','創作工坊','健康生活'];
 demoNames.push(...Array.from({length:108},(_,i)=>`捲動測試房${String(i).padStart(3,'0')}`));
 const demoRooms=demoNames.map((name,i)=>({id:`11111111-1111-4111-8111-${String(i).padStart(12,'0')}`,name}));
 const categories=['綜合','技術','休閒'];
 for(let i=0;i<categories.length;i++)db.prepare("INSERT INTO codemap(category,code,value) VALUES('room_category',?,?)").run('preview-'+i,categories[i]);
 for(const [i,room] of demoRooms.entries())db.prepare("INSERT INTO rooms(id,category_room_id,name,desc,created_at,updated_at) VALUES(?,?,?,'',CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)").run(room.id,'preview-'+(i%3),room.name);
 browser=await chromium.launch({headless:true,ignoreDefaultArgs:['--hide-scrollbars'],executablePath:process.env.CHROMIUM_EXECUTABLE??'/home/willie/.cache/ms-playwright/chromium-1248/chrome-linux64/chrome'});
 const a=await context(),pa=await a.newPage();
 await a.route('**/api/session',async route=>{await delay(200);await route.continue();});
 await pa.goto(base+'/');await navigation(pa,'登入');assert.equal(await pa.locator('form[action="/logout"]').count(),0);record('匿名首頁水合完成、延遲Session回應後導覽可用');

 await wait(async()=>await pa.locator('.directory-panel').getAttribute('aria-busy')==='false','初始目錄');
 const originalLinks=await pa.locator('.room-directory .room-card').count();
 let held=false;
 await a.route('**/api/rooms?**',async route=>{held=true;await delay(1400);try{await route.fulfill({status:200,contentType:'application/json',body:'[]'});}catch{}});
 await pa.locator('#directory-query').fill('取消測試');await pa.getByRole('button',{name:'搜尋聊天室',exact:true}).click();
 await wait(async()=>held,'請求確實送出');await pa.locator('#directory-query').fill('保留的新草稿');
 await pa.getByRole('button',{name:'取消查詢',exact:true}).click({timeout:3000});
 await wait(async()=>await pa.locator('.directory-panel').getAttribute('aria-busy')==='false','取消立即復原');
 assert.match(await pa.locator('.directory-cancelled').innerText(),/保留上次清單/);assert.equal(await pa.locator('#directory-query').inputValue(),'保留的新草稿');
 await delay(1700);assert.equal(await pa.locator('.room-directory .room-card').count(),originalLinks);assert.equal(await pa.locator('#directory-query').inputValue(),'保留的新草稿');
 record('CHAT-66-取消已送出的查詢保留清單草稿且忽略晚回應');
 await a.unroute('**/api/rooms?**');await pa.getByRole('button',{name:'重新整理',exact:true}).click();await wait(async()=>await pa.locator('.directory-panel').getAttribute('aria-busy')==='false','取消後可重新查詢');
 const initial=await context(),ip=await initial.newPage();let initialHeld=false;
 await initial.route('**/api/room-categories',async route=>{initialHeld=true;await delay(1400);try{await route.continue();}catch{}});
 await ip.goto(base+'/');await wait(async()=>initialHeld,'首筆請求');await ip.getByRole('button',{name:'取消查詢',exact:true}).click({timeout:3000});
 assert.match(await ip.locator('.directory-count').innerText(),/清單尚未取得/);assert.equal(await ip.locator('.directory-empty').count(),0);
 await delay(1600);assert.equal(await ip.locator('.directory-empty').count(),0);await initial.unroute('**/api/room-categories');
 await ip.getByRole('button',{name:'重新整理',exact:true}).click();await wait(async()=>await ip.locator('.directory-panel').getAttribute('aria-busy')==='false','首次取消後重新讀取');
 assert.match(await ip.locator('.directory-count').innerText(),/顯示/);await ip.screenshot({path:join(out,'day66-cancel-recovered.png')});await initial.close();
 record('CHAT-66-首次載入取消不假報空清單且可重新取得');
 const ca='tech-a',cb='tech-b',ce='empty-category';
 for(const [code,label]of [[ca,'同名分類'],[cb,'同名分類'],[ce,'尚無房間分類']])db.prepare("INSERT INTO codemap(category,code,value) VALUES('room_category',?,?)").run(code,label);
 const ra='22222222-2222-4222-8222-000000000001',rb='22222222-2222-4222-8222-000000000002';
 for(const [id,category,name]of [[ra,ca,'代碼分類甲'],[rb,cb,'代碼分類乙']])db.prepare("INSERT INTO rooms(id,category_room_id,name,desc,created_at,updated_at) VALUES(?,?,?,'',CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)").run(id,category,name);
 await pa.reload();await navigation(pa,'登入');await wait(async()=>await pa.locator(`[data-category="${ce}"]`).count()===1,'空分類也在獨立目錄');
 await pa.locator(`[data-category="${ca}"]`).click();await wait(async()=>await pa.locator(`[data-category="${ca}"]`).getAttribute('aria-pressed')==='true','分類A查詢完成');assert.equal(await pa.locator(`a[href="/room/${ra}"]`).count(),1);assert.equal(await pa.locator(`a[href="/room/${rb}"]`).count(),0);record('CHAT-64-同中文名稱分類按真正代碼隔離');
 const newCategoryRoom='22222222-2222-4222-8222-000000000003';db.prepare("INSERT INTO rooms(id,category_room_id,name,desc,created_at,updated_at) VALUES(?,?,?,'',CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)").run(newCategoryRoom,ca,'分類更新%_新房');
 await pa.locator('#directory-query').fill('%_');await pa.getByRole('button',{name:'搜尋聊天室',exact:true}).click();await wait(async()=>await pa.locator(`a[href="/room/${newCategoryRoom}"]`).count()===1,'伺服器取得新分類房間');assert.equal(await pa.locator(`a[href="/room/${ra}"]`).count(),0);record('CHAT-64-名稱與分類共同送到Server且LIKE特殊字元字面比對');
 await pa.getByRole('button',{name:'清除搜尋',exact:true}).click();await wait(async()=>await pa.locator('#directory-query').inputValue()===''&&await pa.locator(`a[href="/room/${ra}"]`).count()===1,'清除保留分類');assert.equal(await pa.locator(`a[href="/room/${rb}"]`).count(),0);record('CHAT-64-清除送空名稱並保留分類代碼');
 const failedCat=async route=>route.fulfill({status:503,body:''});await pa.route('**/api/rooms?*',failedCat);await pa.locator(`[data-category="${cb}"]`).click();await wait(async()=>await pa.locator('.directory-error').count()===1,'分類查詢失敗');assert.equal(await pa.locator(`a[href="/room/${ra}"]`).count(),1);assert.equal(await pa.locator(`[data-category="${ca}"]`).getAttribute('aria-pressed'),'true');await pa.unroute('**/api/rooms?*',failedCat);await pa.getByRole('button',{name:'重試',exact:true}).click();await wait(async()=>await pa.locator(`a[href="/room/${rb}"]`).count()===1&&await pa.locator('.directory-error').count()===0,'重試捕捉原分類B');record('CHAT-64-分類失敗保原快照與分類，Retry原請求');
 await pa.locator(`[data-category="${ce}"]`).click();await wait(async()=>await pa.getByText('沒有符合條件的聊天室。',{exact:true}).count()===1,'空分類不是全站空房');record('CHAT-64-空分類不冒稱全站無房間');await pa.getByRole('button',{name:'清除篩選',exact:true}).click();await wait(async()=>await pa.locator(`a[href="/room/${rb}"]`).count()===1,'回全部分類');
 const searchBounds=await pa.locator('.directory-search').boundingBox(),categoryBounds=await pa.locator('.directory-categories').boundingBox();assert(categoryBounds.y>=searchBounds.y+searchBounds.height);await pa.screenshot({path:join(out,'chat-day64-directory.png'),fullPage:true});record('CHAT-64-首頁查詢與分類分行');
 for(const id of [ra,rb,newCategoryRoom])db.prepare('DELETE FROM rooms WHERE id=?').run(id);for(const code of [ca,cb,ce])db.prepare("DELETE FROM codemap WHERE category='room_category' AND code=?").run(code);await pa.reload();await navigation(pa,'登入');
 await wait(async()=>await pa.locator('.directory-categories button').count()>=4,'首頁分類');
 const searchBox=await pa.locator('.directory-search').boundingBox(),categoryBox=await pa.locator('.directory-categories').boundingBox();assert(searchBox&&categoryBox);assert(categoryBox.y>=searchBox.y+searchBox.height+8,'首頁查詢和分類必須各自一行');record('首頁桌面查詢一行、分類另一行');
 await pa.screenshot({path:join(out,phase+'-default-home.png'),fullPage:true});
 await pa.setViewportSize({width:360,height:800});assert(await pa.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));const smallSearch=await pa.locator('.directory-search').boundingBox(),smallCategories=await pa.locator('.directory-categories').boundingBox();assert(smallCategories.y>=smallSearch.y+smallSearch.height);record('首頁360px仍分行且無水平溢出');await pa.setViewportSize({width:1280,height:900});
 await signup(pa,'hydrate_a');await login(pa,'hydrate_a');
 let nameReads=0;await a.route('**/api/rooms/'+demoRooms[0].id,async route=>{nameReads++;if(nameReads===1)await route.fulfill({status:503,body:''});else await route.continue();});
 await pa.goto(base+'/room/'+demoRooms[0].id);await wait(async()=>await pa.locator('.conversation h2').innerText().then(x=>x.includes('無法取得房間名稱')),'房名故障狀態');
 await pa.locator('.chat-composer textarea').fill('房名重試仍保留');
 await pa.getByRole('button',{name:'重試房間名稱',exact:true}).click({timeout:3000});
 await wait(async()=>await pa.locator('.conversation h2').innerText()==='房間 '+demoRooms[0].name,'重試讀回正式房名');
 assert.equal(await pa.locator('.chat-composer textarea').inputValue(),'房名重試仍保留');assert.equal(nameReads,2);assert.equal(await pa.getByRole('button',{name:'重試房間名稱',exact:true}).count(),0);
 await pa.screenshot({path:join(out,'day67-room-name-retry.png')});record('CHAT-67-房名失敗精確重試且不清草稿或重建頁面');await a.unroute('**/api/rooms/'+demoRooms[0].id);await pa.goto(base+'/account');await pa.locator('form[action="/account/rooms"] [name=name]').waitFor();

 await pa.locator('form[action="/account/rooms"] [name=name]').fill('綜合聊天室');
 await Promise.all([pa.waitForResponse(r=>r.url().endsWith('/account/rooms')&&r.request().method()==='POST'),pa.locator('form[action="/account/rooms"] button').click()]);
 const room=db.prepare('SELECT id FROM rooms WHERE name=?').get('綜合聊天室').id;
 await pa.goto(base+'/');await navigation(pa,'管理');await wait(async()=>await pa.locator('form[action="/logout"] [name=csrf]').count()===1,'登出權杖');record('登入後導覽切換管理與真CSRF登出表單');

 await wait(async()=>await (await pa.locator('.room-directory').innerText()).includes('綜合聊天室'),'初次目錄');
 const directoryRequests=[];pa.on('request',r=>{if(new URL(r.url()).pathname==='/api/rooms')directoryRequests.push(new URL(r.url()).searchParams.get('q'));});
 const creationPage=await a.newPage();await creationPage.goto(base+'/account');
 await creationPage.locator('form[action="/account/rooms"] [name=name]').fill('即時%_房間');
 await Promise.all([creationPage.waitForResponse(r=>r.url().endsWith('/account/rooms')&&r.request().method()==='POST'),creationPage.locator('form[action="/account/rooms"] button').click()]);
 const newId=db.prepare('SELECT id FROM rooms WHERE name=?').get('即時%_房間').id;
 await pa.locator('#directory-query').fill('%_');
 assert.equal(await pa.locator(`a[href="/room/${newId}"]`).count(),0);
 await pa.getByRole('button',{name:'搜尋聊天室',exact:true}).click();
 await wait(async()=>await pa.locator(`a[href="/room/${newId}"]`).count()===1,'新建房間搜尋');
 assert.equal(directoryRequests.at(-1),'%_');assert.equal(await pa.locator(`a[href="/room/${room}"]`).count(),0);record('首頁向伺服器搜尋新建房間與LIKE字面值，草稿不冒充查詢成功');
 const failDirectory=async route=>route.fulfill({status:503,body:''});await pa.route('**/api/rooms?*',failDirectory);
 await pa.getByRole('button',{name:'清除搜尋',exact:true}).click();await wait(async()=>await pa.locator('.directory-error').count()===1,'清除失敗');assert.equal(await pa.locator('#directory-query').inputValue(),'%_');assert.equal(await pa.locator(`a[href="/room/${newId}"]`).count(),1);record('失敗保留草稿、最近成功查詢與房間連結');
 await pa.unroute('**/api/rooms?*',failDirectory);
 await pa.locator('#directory-query').fill('新輸入');await pa.getByRole('button',{name:'重試',exact:true}).click();
 await wait(async()=>await pa.locator('.directory-error').count()===0 && await pa.locator(`a[href="/room/${room}"]`).count()===1,'重試原失敗條件');assert.equal(directoryRequests.at(-1),'');assert.equal(await pa.locator('#directory-query').inputValue(),'');record('重試捕捉原失敗條件並成功更新快照');
 let release;await pa.route('**/api/rooms?*',async route=>                                  {await new Promise(resolve=>release=resolve);await route.continue();});
 await pa.getByRole('button',{name:'重新整理',exact:true}).click();await wait(async()=>!!release,'延遲請求');await pa.locator('#directory-query').fill('晚到回應保留的新草稿');release();
 await wait(async()=>await pa.getByRole('button',{name:'搜尋聊天室',exact:true}).isEnabled(),'刷新完成');await pa.unroute('**/api/rooms?*');assert.equal(await pa.locator('#directory-query').inputValue(),'晚到回應保留的新草稿');record('刷新晚回應不抹除請求後的新輸入');
 await pa.locator('#directory-query').fill('不存在的房間');await pa.getByRole('button',{name:'搜尋聊天室',exact:true}).click();await wait(async()=>await pa.getByText('沒有符合條件的聊天室。',{exact:true}).count()>0,'搜尋空結果');await pa.screenshot({path:join(out,'green-directory.png'),fullPage:true});record('搜尋零結果可見並保留查詢條件');
 await creationPage.close();
 db.prepare("UPDATE rooms SET name=? WHERE id=?").run("新成立聊天室",newId);
 const failed=async route=>route.fulfill({status:503,body:''});await a.route('**/api/session',failed);await pa.reload();await navigation(pa,'登入');assert.match(await pa.locator('.site-status').innerText(),/無法確認登入狀態/);assert.equal(await pa.locator('form[action="/logout"]').count(),0);await a.unroute('**/api/session',failed);record('Session故障不顯示無效登出，重載可恢復');
 const second=await context(),secondPage=await second.newPage();await login(secondPage,'hydrate_a');
 const b=await context(),pb=await b.newPage();await signup(pb,'hydrate_b');await login(pb,'hydrate_b','/room/'+room);
 await pa.goto(base+'/room/'+room);await wait(async()=>await pa.getByText('已連線',{exact:true}).count()>0&&await pb.getByText('已連線',{exact:true}).count()>0,'雙人連線');record('房間頁水合完成並建立雙人WebSocket');
 let memberFault=true,memberReads=0;await a.route('**/api/rooms/*/members',async route=>{memberReads++;if(memberFault)await route.fulfill({status:503,body:''});else await route.continue();});
 await pa.locator('.chat-composer textarea').fill('在線名單重試保留草稿');
 await pa.getByRole('button',{name:'重試在線人員',exact:true}).waitFor({timeout:7500});
 await pa.screenshot({path:join(out,'day68-members-error.png')});
 memberFault=false;await pa.getByRole('button',{name:'重試在線人員',exact:true}).click();
 await wait(async()=>await pa.locator('.room-members [role=status]').innerText()==='目前在線','在線人員手動重試');
 assert(memberReads>=2);assert.equal(await pa.locator('.chat-composer textarea').inputValue(),'在線名單重試保留草稿');
 assert.equal(await pa.getByRole('button',{name:'重試在線人員',exact:true}).count(),0);await pa.screenshot({path:join(out,'day68-members-retry.png')});record('CHAT-68-在線人員失敗手動重試且保留聊天草稿與連線');await a.unroute('**/api/rooms/*/members');await pa.locator('.chat-composer textarea').fill('');

 const popular=pa.locator('.popular-rooms');await wait(async()=>await popular.locator('li').count()>0,'取得熱門列表');writeFileSync(join(out,'popular-100-api.json'),JSON.stringify({expected_limit:100,actual_count:(await (await fetch(base+'/api/rooms?popular=true')).json()).length},null,2));await wait(async()=>await popular.locator('li').count()===100,'熱門前100列表');assert.equal(await popular.locator('input,button,select,form').count(),0);assert.equal(await popular.locator('.room-category').count(),0);record('內頁只顯示100間名稱與在線人數，沒有查詢控制');
 const scrollList=popular.locator('.room-results');const scrollState=await scrollList.evaluate(el=>({height:el.clientHeight,total:el.scrollHeight,overflow:getComputedStyle(el).overflowY}));assert(scrollState.total>scrollState.height+100);assert.equal(scrollState.overflow,'auto');assert.notEqual(await scrollList.evaluate(el=>getComputedStyle(el).scrollbarColor),'auto');
 const messageBefore=await pa.locator('.message-history').boundingBox();await scrollList.evaluate(el=>el.scrollTop=el.scrollHeight);assert(await scrollList.evaluate(el=>el.scrollTop)>0);const messageAfter=await pa.locator('.message-history').boundingBox();assert.deepEqual(messageAfter,messageBefore);await scrollList.hover();await pa.screenshot({path:join(out,phase+'-100-desktop-scrolled.png'),fullPage:true});await scrollList.evaluate(el=>el.scrollTop=0);record('100間列表桌面獨立捲動，訊息區位置與高度不變');

 async function openPresence(p,id){await p.evaluate(id=>new Promise((resolve,reject)=>{const ws=new WebSocket(`ws://${location.host}/ws/${id}`);window.previewSockets??=[];window.previewSockets.push(ws);ws.onopen=()=>resolve(true);ws.onerror=()=>reject(Error('在線測試連線失敗'));}),id);}
 async function currentPopular(){const response=await fetch(base+'/api/rooms?popular=true');assert.equal(response.status,200);return response.json();}
 const hot=demoRooms[0].id;await openPresence(pa,hot);await openPresence(secondPage,hot);await wait(async()=>(await currentPopular()).some(x=>x.id===hot&&x.online_count===1),'會員去重');record('同會員不同Session及分頁只算1人');
 await openPresence(pb,hot);const c=await context(),pc=await c.newPage();await signup(pc,'hydrate_c');await login(pc,'hydrate_c');await openPresence(pc,hot);
 await wait(async()=>(await currentPopular())[0]?.id===hot&&(await currentPopular())[0]?.online_count===3,'3名在線會員排序');await wait(async()=>await popular.locator('li').first().locator('a').getAttribute('href')==='/room/'+hot&&(await popular.locator('li').first().innerText()).includes('3 人在線'),'熱門列表自動更新');const ranked=await currentPopular();assert.equal(ranked.length,100);assert(ranked.every((x,i)=>i===0||ranked[i-1].online_count>=x.online_count));record('依真正在線人數降冪排序並限制前100間');

 for(const [page,message] of [[pa,'歡迎來到綜合聊天室，今天想聊什麼？'],[pb,'畫面把空間留給對話，小螢幕也能舒服閱讀。']]) {
  await page.locator('textarea[placeholder="輸入訊息"]').fill(message);await page.getByRole('button',{name:'傳送',exact:true}).click();
  await wait(async()=>await pa.locator('.message-history li').filter({hasText:message}).count()===1,'預設圖示意對話');
 }
 await pa.reload();await navigation(pa,'管理');await wait(async()=>await pa.getByText('已連線',{exact:true}).count()>0&&await pa.locator('.message-history li').count()>=2,'示意歷史讀回');
 const roster=pa.locator('.room-members');
 await wait(async()=>await roster.locator('li').count()===2,'右側真在線人員');
 assert.deepEqual((await roster.locator('.member-name').allTextContents()).sort(),['小安','小晴']);assert.equal(await roster.locator('button:disabled').count(),2);
 assert.equal((await fetch(base+'/api/rooms/'+room+'/members')).status,401);
 const rosterReply=await pa.request.get(base+'/api/rooms/'+room+'/members');assert.equal(rosterReply.headers()['cache-control'],'no-store');assert.deepEqual((await rosterReply.json()).sort(),['小安','小晴']);
 await assert.doesNotReject(()=>pa.getByRole('button',{name:'語音通話（尚未接通）',exact:true}).isDisabled().then(value=>assert(value)));
 assert(await pa.getByRole('button',{name:'視訊通話（尚未接通）',exact:true}).isDisabled());
 await pa.locator('.chat-more summary').click();assert.equal(await pa.getByRole('button',{name:'AI 助理',exact:true}).count(),0);assert(await pa.getByRole('button',{name:'文字顏色',exact:true}).isDisabled());assert.equal(await pa.getByRole('button',{name:'提示音',exact:true}).count(),0);await pa.screenshot({path:join(out,phase+'-room-more.png'),fullPage:true});await pa.locator('.chat-more summary').click();
 record('右側成員按鈕與匿名拒讀、語音視訊未開放、更多文字色；依本次確認隱藏AI及提示音');
 await pa.setViewportSize({width:390,height:740});
 assert(!(await roster.isVisible()));await pa.getByRole('button',{name:'在線人員',exact:true}).click();assert(await roster.isVisible());await pa.getByRole('button',{name:'在線人員',exact:true}).click();assert(!(await roster.isVisible()));
 const contentBox=await pa.locator('.message-history').boundingBox(),conversationBox=await pa.locator('.conversation').boundingBox(),composerBox=await pa.locator('.chat-composer').boundingBox();
 assert(contentBox&&conversationBox&&composerBox);assert(contentBox.height>=conversationBox.height*.55,'小螢幕訊息區至少佔對話區55%');assert(composerBox.height<=48,'輸入列預設一行');assert(composerBox.y+composerBox.height<=740,'輸入列在視窗內');assert(await pa.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
 const toggle=pa.getByRole('button',{name:'熱門聊天室',exact:true});assert.equal(await toggle.getAttribute('aria-expanded'),'false');assert.equal(await popular.isVisible(),false);await toggle.click();await wait(async()=>await popular.isVisible(),'展開熱門列表');assert.equal(await popular.locator('li').count(),100);const mobileMetrics=await pa.locator('.room-sidebar').evaluate(el=>Array.from(el.querySelectorAll('.sidebar-content,.room-results')).map(n=>({class:n.className,height:n.clientHeight,total:n.scrollHeight,overflow:getComputedStyle(n).overflowY})));writeFileSync(join(out,'mobile-scroll-metrics.json'),JSON.stringify(mobileMetrics,null,2));let mobileScroll=popular.locator('.room-results');if(!(await mobileScroll.evaluate(el=>el.scrollHeight>el.clientHeight+100)))mobileScroll=pa.locator('.room-sidebar .sidebar-content');assert(await mobileScroll.evaluate(el=>el.scrollHeight>el.clientHeight+100));await mobileScroll.evaluate(el=>el.scrollTop=el.scrollHeight);assert(await mobileScroll.evaluate(el=>el.scrollTop)>0);await pa.screenshot({path:join(out,phase+'-100-mobile-scrolled.png'),fullPage:true});record('100間列表手機抽屜可捲至末端');await toggle.click();await wait(async()=>!(await popular.isVisible()),'收合熱門列表');record('390x740訊息區優先、輸入列單行，熱門列表可收合');
 await pa.screenshot({path:join(out,phase+'-default-room-mobile.png'),fullPage:true});

 for(const viewport of [{width:1024,height:600},{width:1366,height:768}]){
  await pa.setViewportSize(viewport);await wait(async()=>await popular.isVisible()&&await roster.isVisible(),'筆電雙側列表');const history=await pa.locator('.message-history').boundingBox(),section=await pa.locator('.conversation').boundingBox(),input=await pa.locator('.chat-composer').boundingBox();assert(history.height>=section.height*.70,'筆電訊息區至少佔對話區70%');assert(input.height<=48&&input.y+input.height<=viewport.height);assert(await pa.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
  assert((await pa.locator('.site-header').boundingBox()).height<=52,'桌面導覽列不超過52px');assert((await pa.locator('.conversation-heading').boundingBox()).height<=40,'房間資訊與狀態同列');
  if(viewport.width===1024)await pa.screenshot({path:join(out,phase+'-default-room-laptop.png'),fullPage:true});
 }record('1024x600與1366x768筆電皆保留主要訊息區，控制列不擠占內容');
 await pa.setViewportSize({width:1280,height:900});await wait(async()=>await popular.isVisible()&&await popular.locator('li').count()===100,'桌面熱門列表恢復');
 const desktopHistory=await pa.locator('.message-history').boundingBox(),desktopConversation=await pa.locator('.conversation').boundingBox();assert(desktopHistory.height>=desktopConversation.height*.6);record('桌面訊息區至少佔對話區60%，歷史操作縮入標頭');
 await pa.screenshot({path:join(out,phase+'-default-room.png'),fullPage:true});
 await pa.setViewportSize({width:390,height:740});const composerInput=pa.locator('textarea[placeholder="輸入訊息"]');const countBefore=db.prepare('SELECT count(*) n FROM messages').get().n;
 await composerInput.fill('訊息區保留主要空間。\n輸入框需要時才長高。\n送出後回到一行。\n列表不會擋住對話。');const grown=await composerInput.boundingBox();assert(grown.height>=70&&grown.height<=96);await composerInput.press('Shift+Enter');assert.equal(db.prepare('SELECT count(*) n FROM messages').get().n,countBefore);await composerInput.press('Enter');await wait(async()=>await composerInput.inputValue()===''&&(await composerInput.boundingBox()).height<=48&&db.prepare('SELECT count(*) n FROM messages').get().n===countBefore+1,'送出後恢復一行');record('小螢幕多行輸入限高、Shift+Enter不傳送，Enter送出後回單行');await pa.setViewportSize({width:1280,height:900});

 const logoutC=await c.newPage();await logoutC.goto(base+'/');await navigation(logoutC,'管理');await wait(async()=>await logoutC.locator('form[action="/logout"] [name=csrf]').count()===1,'C登出權杖');await logoutC.locator('form[action="/logout"] button').click();await wait(async()=>(await currentPopular()).find(x=>x.id===hot)?.online_count===2,'撤銷在線人數');record('會員登出撤銷連線後即移除在線人數');
 const failingPopular=async route=>route.fulfill({status:503,body:''});await pa.route('**/api/rooms?popular=true',failingPopular);await wait(async()=>(await popular.innerText()).includes('暫時無法更新'),'熱門故障提示');assert.equal(await popular.locator('li').count(),100);await pa.unroute('**/api/rooms?popular=true',failingPopular);await wait(async()=>!(await popular.innerText()).includes('暫時無法更新'),'熱門自動恢復');record('熱門讀取失敗保留列表並自動恢復，無手動查詢按鈕');

 const marker='水合修正後的真UI訊息';await pa.locator('textarea[placeholder="輸入訊息"]').fill(marker);await pa.getByRole('button',{name:'傳送',exact:true}).click();await wait(async()=>await pb.locator('.message-history li').filter({hasText:marker}).count()===1,'另一瀏覽器收到訊息');assert.equal(db.prepare('SELECT count(*) n FROM messages WHERE content=?').get(marker).n,1);record('真UI送出、另一瀏覽器顯示、SQLite只寫一次');
 await pa.getByRole('button',{name:'重新載入歷史',exact:true}).click();await wait(async()=>await pa.locator('.message-history li').filter({hasText:marker}).count()===1,'歷史重載');record('歷史重載保留訊息且不重複');

 await pa.setViewportSize({width:1280,height:900});
 // 透過正式送出路徑累積可捲動訊息。
 for(let i=0;i<32;i++){await pa.locator('.chat-composer textarea').fill('捲動測試 '+i+' 內容'.repeat(12));await pa.getByRole('button',{name:'傳送',exact:true}).click();await wait(async()=>await pa.locator('.message-history li').filter({hasText:'捲動測試 '+i+' '}).count()===1,'真訊息已儲存');}
 await pa.locator('.chat-composer textarea').fill('回到底部仍保留草稿');
 const messageArea=pa.locator('.message-history');assert(await messageArea.evaluate(n=>n.scrollHeight>n.clientHeight+100));
 await messageArea.evaluate(n=>n.scrollTop=0);await pa.getByRole('button',{name:'跳至最新訊息',exact:true}).click();
 assert(await messageArea.evaluate(n=>n.scrollHeight-n.clientHeight-n.scrollTop<=2));assert.equal(await pa.locator('.chat-composer textarea').inputValue(),'回到底部仍保留草稿');
 await pa.screenshot({path:join(out,'day69-latest-messages.png')});record('CHAT-69-真即時訊息捲動後回到最新、保留草稿與既有WS');
 const logoutPage=await a.newPage();await logoutPage.goto(base+'/');await navigation(logoutPage,'管理');await wait(async()=>await logoutPage.locator('form[action="/logout"] [name=csrf]').count()===1,'登出CSRF');
 await Promise.all([logoutPage.waitForURL(base+'/'),logoutPage.locator('form[action="/logout"] button').click()]);await navigation(logoutPage,'登入');await wait(async()=>await pa.getByText('已斷線，請重新進房',{exact:true}).count()>0,'撤銷既有WebSocket');assert.equal((await secondPage.request.get(base+'/api/session')).status(),204);record('真導覽登出撤銷既有WS，另一Session維持');
 await stop(server);server=await start();await pb.reload();await navigation(pb,'管理');await wait(async()=>await pb.locator('.message-history li').filter({hasText:marker}).count()===1&&await pb.getByText('已連線',{exact:true}).count()>0,'重啟恢復');record('程序重啟後水合、Session、歷史與WS恢復');
 assert(responses.some(r=>r.url.endsWith('.wasm')&&r.status===200));assert(!responses.some(r=>r.status>=400));assert.equal(errors.length,0);record('全流程無hydration錯誤與前端資產HTTP錯誤');
 const caption=await pa.locator('.popular-rooms [role=status]').innerText();writeFileSync(join(out,'chat-micro-copy.json'),JSON.stringify({name:'熱門清單上限說明',actual:caption,expected:'依目前在線人數排序，最多100間',status:caption.includes('最多100間')?'PASS':'FAIL'},null,2));
 await pb.screenshot({path:join(out,phase+'-chat.png'),fullPage:true});
}catch(e){
 cases.push({name:'CHAT_R2_REGRESSION',status:'FAIL',error:String(e.stack)});console.error(e);process.exitCode=1;
 if(browser)for(const c of contexts)for(const p of c.pages())try{writeFileSync(join(out,phase+'-failure.html'),await p.content());await p.screenshot({path:join(out,phase+'-failure.png'),fullPage:true});}catch{}
}finally{
 for(const c of contexts)await c.close();await browser?.close();await stop(server);db.close();
 writeFileSync(join(out,phase+'-results.json'),JSON.stringify({phase,fixture,cases,errors,assets:responses},null,2)+'\n');writeFileSync(join(out,phase+'-server.log'),serverLog);
}