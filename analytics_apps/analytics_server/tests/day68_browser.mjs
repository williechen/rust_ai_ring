import assert from 'node:assert/strict';
import {spawn,execFileSync} from 'node:child_process';
import {once} from 'node:events';
import {createRequire} from 'node:module';
import {DatabaseSync} from 'node:sqlite';
import {randomUUID} from 'node:crypto';
import {mkdirSync,writeFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import {dirname,join,resolve} from 'node:path';
import {setTimeout as delay} from 'node:timers/promises';
const w=dirname(fileURLToPath(import.meta.url)),root=process.env.DAY68_PRODUCT_ROOT??join(w,'candidate-source'),out=resolve(process.env.PROBE_OUTPUT_DIR??join(w,'evidence'));
const {chromium}=createRequire(import.meta.url)('playwright');
const browser=await chromium.launch({headless:true,executablePath:'/home/willie/.cache/ms-playwright/chromium-1248/chrome-linux64/chrome'});
const rows=[],servers=[];
const record=(name,detail={})=>{rows.push({name,status:'PASS',...detail});console.log(JSON.stringify(rows.at(-1)));};
async function start(project,env,port){
 assert(await fetch(`http://127.0.0.1:${port}/health`).then(()=>false,()=>true),'測試埠須空閒');
 const binary=process.env.ANALYTICS_BINARY??resolve(w,`../day65-acceptance-2026-11-18/target-${project}/debug/${project}-server`);
 const server=spawn(binary,[],{cwd:root,env:{...process.env,...env},stdio:['ignore','pipe','pipe']});servers.push(server);let logs='';server.stdout.on('data',b=>logs+=b);server.stderr.on('data',b=>logs+=b);
 server.on('exit',()=>writeFileSync(join(out,project+'-browser-server.log'),logs));
 for(let i=0;i<150;i++){assert.equal(server.exitCode,null,logs);if(await fetch(`http://127.0.0.1:${port}/`+(project==='chat'?'health':'healthz')).then(r=>r.ok,()=>false))return server;await delay(100);}
 throw Error('伺服器啟動逾時 '+project+logs);
}
async function stop(s){if(s.exitCode===null&&s.signalCode===null){const end=once(s,'exit');s.kill('SIGTERM');await end;}}
try{
 const abase='http://127.0.0.1:3100',url=`postgresql://day65_accept@127.0.0.1:55465/${process.env.PROBE_PHASE??'green'}_day67_browser`;
 const aserver=await start('analytics',{ANALYTICS_DATABASE_URL:url,ANALYTICS_CHAT_TOKEN:'fixture-chat',ANALYTICS_PET_TOKEN:'fixture-pet'},3100);
 const ctx=await browser.newContext({acceptDownloads:true}),p=await ctx.newPage();
 async function openLegacy(){const panel=p.locator('.legacy-statistics');if(await panel.count()&&!(await panel.getAttribute('open')!==null))await panel.locator('summary').first().click();}
 async function go(url){await p.goto(url);await openLegacy();}

 async function alogin(pass,target){await go(abase+'/login');await p.locator('[name=username]').fill('admin');await p.locator('[name=password]').fill(pass);await Promise.all([p.waitForURL(abase+target),p.getByRole('button',{name:'登入',exact:true}).click()]);}
 await alogin('admin','/change-password');for(const [key,value] of Object.entries({current_password:'admin',new_password:'Synthetic-analytics-62',confirm_password:'Synthetic-analytics-62',email:'admin62@example.test'}))await p.locator(`form[action="/change-password"] [name=${key}]`).fill(value);
 await Promise.all([p.waitForURL(abase+'/login'),p.locator('form[action="/change-password"] button[type=submit]').click()]);await alogin('Synthetic-analytics-62','/dashboard');

 const invalidKind='訊息 & message';const invalidURL=abase+'/dashboard?source=web_chat&event_type='+encodeURIComponent(invalidKind)+'&page_size=25&start=2026-11-10&end=2026-11-09';
 const invalidResponse=await p.goto(invalidURL);assert.equal(invalidResponse.status(),400);assert.equal(await p.getByRole('heading',{name:'日期範圍無效',exact:true}).count(),1);
 await p.screenshot({path:join(out,'day68-date-recovery.png'),fullPage:true});await p.getByRole('link',{name:'回到查詢',exact:true}).click();await p.waitForURL(u=>!u.searchParams.has('start'));
 const recoveredURL=new URL(p.url());assert.equal(recoveredURL.searchParams.get('source'),'web_chat');assert.equal(recoveredURL.searchParams.get('event_type'),invalidKind);assert.equal(recoveredURL.searchParams.get('page_size'),'25');assert.equal(recoveredURL.searchParams.get('page'),'1');assert.equal(await p.getByLabel('事件類型（模糊查詢）',{exact:true}).inputValue(),invalidKind);
 record('ANALYTICS-68-無效日期400提供恢復入口，回到有效預設範圍並保留來源／模糊文字／筆數');await go(abase+'/dashboard');
 const exact='kind.中文 &';
 const privateKind='pet.private.only';
 const privateResponse=await fetch(abase+'/api/v1/events',{method:'POST',headers:{'Content-Type':'application/json',Authorization:'Bearer fixture-pet'},body:JSON.stringify({schema_version:1,source_app:'desktop_pet',event_id:randomUUID(),event_type:privateKind})});assert.equal(privateResponse.status,201);
 for(const source of ['web_chat','desktop_pet'])for(let i=0;i<27;i++){
  const r=await fetch(abase+'/api/v1/events',{method:'POST',headers:{'Content-Type':'application/json',Authorization:'Bearer '+(source==='web_chat'?'fixture-chat':'fixture-pet')},body:JSON.stringify({schema_version:1,source_app:source,event_id:randomUUID(),event_type:i===25?exact:i===26?'=1+1':`kind.${String(i).padStart(2,'0')}`})});assert.equal(r.status,201);
 }

async function ingestTime(source,time,id=randomUUID()) {return fetch(abase+'/api/v1/events',{method:'POST',headers:{'Content-Type':'application/json',Authorization:'Bearer '+(source==='web_chat'?'fixture-chat':'fixture-pet')},body:JSON.stringify({schema_version:1,source_app:source,event_id:id,event_type:'kind.00',occurred_at:time})});}
 const datedId=randomUUID();assert.equal((await ingestTime('web_chat','2026-11-08T23:59:59Z',datedId)).status,201);assert.equal((await ingestTime('web_chat','2026-11-08T23:59:59.000Z',datedId)).status,200);assert.equal((await ingestTime('web_chat','2026-11-09T00:00:00Z',datedId)).status,409);
 for(const time of ['2026-11-09T00:00:00Z','2026-11-09T12:00:00Z'])assert.equal((await ingestTime('web_chat',time)).status,201);assert.equal((await ingestTime('desktop_pet','2026-11-09T08:00:00Z')).status,201);record('ANALYTICS-64-發生時間真HTTP持久化與重送／異時衝突');
 const dailyUrl=abase+'/dashboard?source=web_chat&event_type=kind.00&start=2026-11-08&end=2026-11-11';await p.goto(dailyUrl);assert.deepEqual(await p.locator('.daily-column strong').allTextContents(),['1','2','0']);assert.match(await p.locator('.daily-warning').innerText(),/1 筆缺少發生時間/);assert.equal(await p.locator('.legacy-statistics').getAttribute('open'),null);await p.screenshot({path:join(out,'analytics-day64-daily.png'),fullPage:true});record('ANALYTICS-64-UTC跨日分組、成功空日0與舊日期未知分開');

 await p.getByRole('link',{name:'查看 2026-11-09 每日分析',exact:true}).click();
 await p.waitForURL(u=>u.searchParams.get('start')==='2026-11-09'&&u.searchParams.get('end')==='2026-11-10');await p.locator('.daily-column strong').first().waitFor();let dayURL=new URL(p.url());assert.equal(dayURL.searchParams.get('source'),'web_chat');assert.equal(dayURL.searchParams.get('event_type'),'kind.00');assert.equal(dayURL.searchParams.get('start'),'2026-11-09');assert.equal(dayURL.searchParams.get('end'),'2026-11-10');assert.equal(dayURL.searchParams.get('page'),'1');assert.deepEqual(await p.locator('.daily-column strong').allTextContents(),['2']);
 record('ANALYTICS-66-點日期保留來源精確種類並縮成UTC單日');
 await p.goto(dailyUrl);await p.getByRole('link',{name:'查看 2026-11-10 每日分析',exact:true}).click();
 await p.waitForURL(u=>u.searchParams.get('start')==='2026-11-10');await p.locator('.daily-report details tbody tr').first().waitFor({state:'attached'});dayURL=new URL(p.url());assert.equal(dayURL.searchParams.get('end'),'2026-11-11');assert.equal(await p.locator('.daily-column').count(),0);assert.match(await p.locator('.daily-report').innerText(),/不以接收日期或假零值補圖/);assert.deepEqual(await p.locator('.daily-report details tbody td').allTextContents(),['2026-11-10','0']);
const selectedCSV=await p.request.get(abase+await p.locator('.daily-export').getAttribute('href'));assert.equal(selectedCSV.status(),200);const selectedBody=await selectedCSV.text();assert(selectedBody.split(/\r?\n/).some(line=>line.endsWith('"2026-11-10",0,')));assert(!selectedBody.split(/\r?\n/).some(line=>line.includes('"2026-11-09",')));
 await p.screenshot({path:join(out,'analytics-day66-single-day.png'),fullPage:true});record('ANALYTICS-66-末日使用排他結束邊界且CSV同範圍');await p.goto(dailyUrl);

 await p.goto(abase+'/dashboard?source=web_chat&event_type='+encodeURIComponent(exact)+'&page=99&page_size=50&start=2026-11-08&end=2026-11-11');
 await p.getByRole('link',{name:'最近7天',exact:true}).click({timeout:3000});await p.waitForURL(u=>!u.searchParams.has('start'));
 const recentURL=new URL(p.url());assert.equal(recentURL.searchParams.get('source'),'web_chat');assert.equal(recentURL.searchParams.get('event_type'),exact);assert.equal(recentURL.searchParams.get('page_size'),'50');assert.equal(recentURL.searchParams.get('page'),'1');
 const begin=await p.locator('.report-filters [name=start]').inputValue(),end=await p.locator('.report-filters [name=end]').inputValue();assert.equal((Date.parse(end)-Date.parse(begin))/86400000,7);assert.equal(await p.locator('.daily-report details tbody tr').count(),7);
 await p.screenshot({path:join(out,'analytics-day67-recent-week.png'),fullPage:true});record('ANALYTICS-67-最近7天重設UTC日期保留UTF8精確種類來源筆數與第1頁');await p.goto(dailyUrl);
 await p.getByLabel('起日（UTC）',{exact:true}).fill('2026-11-09');await Promise.all([p.waitForURL(u=>u.searchParams.get('start')==='2026-11-09'),p.getByRole('button',{name:'篩選',exact:true}).click()]);assert.deepEqual(await p.locator('.daily-column strong').allTextContents(),['2','0']);assert.equal(new URL(p.url()).hash,'#daily-chart-title');await p.getByLabel('來源',{exact:true}).selectOption('desktop_pet');await Promise.all([p.waitForURL(u=>u.searchParams.get('source')==='desktop_pet'),p.getByRole('button',{name:'篩選',exact:true}).click()]);assert.equal(new URL(p.url()).searchParams.get('start'),'2026-11-09');assert.equal(new URL(p.url()).searchParams.get('end'),'2026-11-11');assert.deepEqual(await p.locator('.daily-column strong').allTextContents(),['1','0']);record('ANALYTICS-64-日期表單與切來源保留精確類型及區間');
 const dailyLink=p.locator('.daily-export');assert.equal(await dailyLink.count(),1);
 const dailyCSV=await p.request.get(abase+await dailyLink.getAttribute('href'));const observedBody=await dailyCSV.text();writeFileSync(join(out,'daily-csv-http.json'),JSON.stringify({url:dailyCSV.url(),status:dailyCSV.status(),content_type:dailyCSV.headers()['content-type'],cache_control:dailyCSV.headers()['cache-control'],body:observedBody},null,2));assert.equal(dailyCSV.status(),200);const dtext=observedBody;assert(dtext.includes('timezone,start_inclusive,end_exclusive,date,total,unknown_dates'));assert(dtext.includes('"2026-11-09",1,1'));assert(dtext.includes('"2026-11-10",0,1'));assert.equal(dailyCSV.headers()['cache-control'],'no-store');record('ANALYTICS-65-每日圖表CSV同來源類型日期與零日');
 const anonymous=await browser.newContext();assert.equal((await anonymous.request.get(abase+'/api/v1/events/web_chat/daily.csv')).status(),401);await anonymous.close();
 assert.equal((await p.request.get(abase+'/api/v1/events/web_chat/daily.csv?start=2026-02-30&end=2026-03-01')).status(),400);record('ANALYTICS-65-匿名拒絕與非法日期400');
 const invalid=await p.request.get(abase+'/dashboard?source=web_chat&start=2026-02-30&end=2026-03-01');assert.equal(invalid.status(),400);const tooLong=await p.request.get(abase+'/dashboard?source=web_chat&start=2026-01-01&end=2026-12-31');assert.equal(tooLong.status(),200);record('ANALYTICS-64-非法日期仍拒絕，長日期區間依最新確認可查');
 await stop(aserver);const restarted=await start('analytics',{ANALYTICS_DATABASE_URL:url,ANALYTICS_CHAT_TOKEN:'fixture-chat',ANALYTICS_PET_TOKEN:'fixture-pet'},3100);await p.goto(dailyUrl);assert.deepEqual(await p.locator('.daily-column strong').allTextContents(),['1','2','0']);record('ANALYTICS-64-程序重啟與Session保留後日統計一致');
 await p.setViewportSize({width:390,height:844});assert(await p.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));await p.screenshot({path:join(out,'analytics-day64-mobile.png'),fullPage:true});await p.setViewportSize({width:1280,height:900});
 for(const size of [10,25,50]){
  await go(abase+`/dashboard?source=web_chat&page=999&page_size=${size}`);assert.match(await p.getByRole('navigation',{name:'報表分頁'}).innerText(),new RegExp(`第 ${Math.ceil(27/size)} / ${Math.ceil(27/size)} 頁`));
 }
 record('ANALYTICS-63-01-真PG與10／25／50筆末頁');
 await go(abase+'/dashboard?source=web_chat&page_size=50');
 assert.equal(await p.locator('#event-type').getAttribute('list'),'report-event-types');
 assert(await p.locator('#report-event-types option').count()>=27);
 const values=await p.locator('#report-event-types option').evaluateAll(nodes=>nodes.map(n=>n.value));assert(values.includes(exact));assert(!values.includes(privateKind));assert(values.includes('=1+1'));record('ANALYTICS-63-字典代碼建議保留UTF8與特殊字元');
 await p.getByLabel('事件類型（模糊查詢）').fill(exact);await p.getByLabel('每頁筆數',{exact:true}).selectOption('50');
 await Promise.all([p.waitForURL(u=>u.searchParams.get('event_type')===exact),p.getByRole('button',{name:'篩選',exact:true}).click()]);
 await p.getByLabel('來源',{exact:true}).selectOption('desktop_pet');await Promise.all([p.waitForURL(u=>u.searchParams.get('source')==='desktop_pet'),p.getByRole('button',{name:'篩選',exact:true}).click()]);
 const target=new URL(p.url());assert.equal(target.searchParams.get('source'),'desktop_pet');assert.equal(target.searchParams.get('event_type'),exact);assert.equal(target.searchParams.get('page_size'),'50');assert.equal(target.searchParams.get('page')??'1','1');assert.equal(target.hash,'#daily-chart-title');assert.equal(await p.locator('.report-table-wrap tbody tr').count(),1);
 assert.equal(await p.getByRole('navigation',{name:'切換報表來源'}).count(),0);assert.equal(await p.getByLabel('來源',{exact:true}).inputValue(),'desktop_pet');
 await p.screenshot({path:join(out,'analytics-day63.png'),fullPage:true});record('ANALYTICS-63-02-表單切換來源保留精確條件筆數與錨點');
 await p.setViewportSize({width:360,height:760});assert(await p.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));record('ANALYTICS-63-03-360px可操作且無橫向溢出');
 const csv=await p.request.get(abase+'/api/v1/events/desktop_pet/export.csv');assert.equal(csv.status(),200);const body=await csv.text();assert.equal(body.trim().split('\n').length,29);assert(body.includes("\"'=1+1\""));record('ANALYTICS-63-04-CSV全量28類型與公式防護');
 await go(abase+'/dashboard?source=desktop_pet&event_type=absent.type&page_size=25');assert(await p.getByText('沒有符合條件的已提交事件',{exact:true}).isVisible());record('ANALYTICS-63-05-空結果與錯誤區分');
 const unknown='unknown.原值 &';await go(abase+'/dashboard?source=web_chat&event_type='+encodeURIComponent(unknown));assert.equal(await p.locator('#event-type').inputValue(),unknown);assert(await p.getByText('沒有符合條件的已提交事件',{exact:true}).isVisible());record('ANALYTICS-63-未知原始wire仍可精確查詢且不被字典替換');
 const deny="INSERT INTO uac_user_acl(user_id,acl_id,acl_filter,created_at) SELECT u.id,a.id,'deny',now() FROM uac_user u CROSS JOIN uac_acl a WHERE lower(u.user_acnt)='admin' AND a.acl_path='/api/v1/events/web_chat' ON CONFLICT(user_id,acl_id) DO UPDATE SET acl_filter='deny';";
 execFileSync('psql',['--no-psqlrc',url,'-v','ON_ERROR_STOP=1','-c',deny],{stdio:'pipe'});
 assert.equal((await p.request.get(abase+'/dashboard?source=web_chat')).status(),403);await go(abase+'/dashboard?source=desktop_pet');assert.equal(await p.locator('#analytics-source option[value=web_chat]').count(),0);record('ANALYTICS-63-06-縮權立即拒絕直接查詢並來源選單隱藏無權項目');
 const machine=await browser.newContext();assert.equal((await machine.request.get(abase+'/api/v1/events/desktop_pet/daily.csv',{headers:{Authorization:'Bearer fixture-pet'}})).status(),401);await machine.close();record('ANALYTICS-65-機器Bearer不可代替人員Session匯出');
 assert.equal((await p.request.get(abase+'/api/v1/events/web_chat/daily.csv')).status(),403);assert.equal((await p.request.get(abase+'/api/v1/events/desktop_pet/daily.csv')).status(),200);
 const exportDeny="INSERT INTO uac_user_acl(user_id,acl_id,acl_filter,created_at) SELECT u.id,a.id,'deny',now() FROM uac_user u CROSS JOIN uac_acl a WHERE lower(u.user_acnt)='admin' AND a.acl_path='/api/v1/events/{source}/export.csv' ON CONFLICT(user_id,acl_id) DO UPDATE SET acl_filter='deny';";
 execFileSync('psql',['--no-psqlrc',url,'-v','ON_ERROR_STOP=1','-c',exportDeny],{stdio:'pipe'});
 assert.equal((await p.request.get(abase+'/api/v1/events/desktop_pet/daily.csv')).status(),403);assert.equal((await p.request.get(abase+'/api/v1/events/desktop_pet/export.csv')).status(),403);await p.goto(abase+'/dashboard?source=desktop_pet');assert.equal(await p.locator('.daily-export').count(),0);record('ANALYTICS-65-新每日CSV同來源與獨立Export拒絕、縮權即時生效');
 await ctx.close();for(const running of servers)await stop(running);
}catch(e){rows.push({name:'HARNESS',status:'FAIL',error:String(e.stack)});console.error(e);process.exitCode=1;}
finally{for(const s of servers)await stop(s);await browser.close();writeFileSync(join(out,'analytics-browser-results.json'),JSON.stringify(rows,null,2)+'\n');}