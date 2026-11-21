from pathlib import Path
import sqlite3,hashlib,subprocess,os,json,time,uuid,signal,requests,base64

W=Path(os.environ.get('DAY68_WORK_ROOT',str(Path(__file__).parent)))
ROOT=Path(os.environ.get('DAY68_PRODUCT_ROOT',str(W/'candidate-source')))
O=Path(os.environ.get('PROBE_OUTPUT_DIR',str(W/'evidence')))
phase=os.environ.get('PROBE_PHASE','green')
data=W/(phase+'-native.sqlite')
assert not data.exists()

A,B=str(uuid.uuid4()),str(uuid.uuid4())
db=sqlite3.connect(data)
db.execute('CREATE TABLE _sqlx_migrations(version BIGINT PRIMARY KEY,description TEXT NOT NULL,installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,success BOOLEAN NOT NULL,checksum BLOB NOT NULL,execution_time BIGINT NOT NULL)')

for p in sorted((ROOT/'pet_apps/migrations').glob('*.sql')):
    raw=p.read_bytes()
    version,desc=p.stem.split('_',1)
    db.executescript(raw.decode())
    db.execute('INSERT INTO _sqlx_migrations(version,description,success,checksum,execution_time) VALUES(?,?,1,?,0)',(int(version),desc.replace('_',' '),hashlib.sha384(raw).digest()))
    db.execute("INSERT INTO pet_owner(id,name) VALUES('local-owner','隔離驗收主人')")
    
for id_,name in [(A,'驗收甲'),(B,'驗收乙')]:
    db.execute("INSERT INTO pet_profile(id,name,species,owner_id,is_default_closure) VALUES(?,?,'tortoise','local-owner',1)",(id_,name))
    
for kind in ['walk','sit']:
    db.execute("INSERT INTO pet_activity(id,pet_id,owner_id,activity_type,is_activity,is_use_ai) VALUES(?,?,'local-owner',?,0,0)",(str(uuid.uuid4()),id_,kind))
    db.execute('INSERT INTO local_pet_selection(slot,pet_id) VALUES(1,?)',(A,));db.commit()

def snapshot():
    return {
        'profiles':db.execute('SELECT id,name,species FROM pet_profile ORDER BY id').fetchall(),
        'extra':db.execute('SELECT * FROM pet_extra ORDER BY id').fetchall(),
        'activity':db.execute('SELECT id,pet_id,is_activity FROM pet_activity ORDER BY id').fetchall()
    }
    
before=snapshot()
rows=[]
driver=None
wm=None
sid=None
api='http://127.0.0.1:4444'

def record(name):
    rows.append({'name':name,'status':'PASS'})
    print(json.dumps(rows[-1],ensure_ascii=False),flush=True)
    
def req(method,path,payload=None):
    r=requests.request(method,api+path,json=payload,timeout=35)
    d=r.json()
    assert r.ok,d
    assert not isinstance(d.get('value'),dict) or 'error' not in d['value'],d
    return d.get('value')

def evaluate(script,*args):
    return req('POST',f'/session/{sid}/execute/sync',{'script':script,'args':list(args)})

def wait(fn,seconds=20):
    end=time.monotonic()+seconds
    while time.monotonic()<end:
        try:
            v=fn()
        except requests.ConnectionError:
            v=None
        if v:
            return v
        time.sleep(.1)
    raise AssertionError('等待Native狀態逾時')

def click(label):
 element=req('POST',f'/session/{sid}/element',{'using':'xpath','value':"//button[normalize-space(.)='"+label+"']"})
 eid=element['element-6066-11e4-a52e-4f735466cecf']
 req('POST',f'/session/{sid}/element/{eid}/click',{})

try:
    log=(O/(phase+'-native-driver.log')).open('w')
    wm=subprocess.Popen(['xfwm4','--sm-client-disable'],stdout=log,stderr=log)
    env=os.environ.copy()
    env.update(PET_AUTHORIZED_ROOT=str(W/'phase-files'),PET_DATABASE_URL='sqlite://'+str(data),WEBKIT_DISABLE_DMABUF_RENDERER='1')
    driver=subprocess.Popen(['tauri-driver','--native-driver','/usr/bin/WebKitWebDriver'],env=env,stdout=log,stderr=log,start_new_session=True)
    
    wait(lambda:requests.get(api+'/status',timeout=1).ok)
    binary=os.environ['PET_BINARY']
    result=req('POST','/session',{
        'capabilities':{
            'alwaysMatch':{
                'browserName':'wry',
                'tauri:options':{'application':binary}
            }
        }
    })
    sid=result['sessionId']
    wait(lambda:evaluate("return !!document.querySelector('#pet-settings-tab');"))
    evaluate("document.querySelector('#pet-settings-tab').click();")
    
    def choose(id):
        evaluate("const s=document.querySelector('.control-pet-picker select');s.value=arguments[0];s.dispatchEvent(new Event('change',{bubbles:true}));",id)
 
    def edit(value):
        evaluate("const n=document.querySelector('[name=settings-name]');if(n.disabled||n.closest('fieldset').disabled)throw Error('設定欄位不可編輯');n.value=arguments[0];n.dispatchEvent(new Event('input',{bubbles:true}));",value)
 
    def dialog():
        return evaluate("return !!document.querySelector('[aria-labelledby=settings-navigation-title]');")
    
    def selected():
        return evaluate("return document.querySelector('.control-pet-picker select').value;")
 
    choose(A);
    wait(lambda:evaluate("return document.querySelector('[name=settings-name]')?.value==='驗收甲';"))
    
    db.execute("CREATE TABLE fixture_settings_writes(pet_id TEXT)");db.execute("CREATE TRIGGER fixture_settings_update AFTER UPDATE ON pet_profile BEGIN INSERT INTO fixture_settings_writes VALUES(new.id); END");db.commit()
 
    evaluate("window.fixtureOriginalSave=window.savePetSettingsChecked;window.savePetSettingsChecked=async(...args)=>({...await window.fixtureOriginalSave(...args),sync_warning:'生活窗通知故障（契約注入）'});")
    
    edit('甲已提交同步待重試');
    click('儲存設定');
    wait(lambda:db.execute('SELECT name FROM pet_profile WHERE id=?',(A,)).fetchone()[0]=='甲已提交同步待重試')
    
    wait(lambda:evaluate("return document.body.textContent.includes('已儲存設定；生活窗同步未完成');"),seconds=8)
    
    record('PET-64-01-真IPC提交後的通知失敗契約明示已儲存')
    (O/(phase+'-save-warning.png')).write_bytes(base64.b64decode(req('GET',f'/session/{sid}/screenshot')))
    count=db.execute('SELECT count(*) FROM fixture_settings_writes').fetchone()[0];
    click('重新同步生活窗');
    wait(lambda:evaluate("return document.body.textContent.includes('已重新同步生活窗');"));
    assert db.execute('SELECT count(*) FROM fixture_settings_writes').fetchone()[0]==count
    assert evaluate("return document.querySelector('[name=settings-name]').value;")=='甲已提交同步待重試';
    record('PET-64-02-真Native重新同步只讀不重寫設定')
    
    evaluate("window.savePetSettingsChecked=window.fixtureOriginalSave;")
    edit('驗收甲');
    click('儲存設定');
    wait(lambda:db.execute('SELECT name FROM pet_profile WHERE id=?',(A,)).fetchone()[0]=='驗收甲');
    before=snapshot()

    edit('過期草稿')
    db.execute("UPDATE pet_profile SET name='其他視窗新值' WHERE id=?",(A,));
    db.commit()
    writes=db.execute('SELECT count(*) FROM fixture_settings_writes').fetchone()[0]
    
    click('儲存設定');
    wait(lambda:evaluate("return document.body.textContent.includes('設定已被其他操作更新');"))
    assert evaluate("return document.querySelector('[name=settings-name]').value;")=='過期草稿'
    assert db.execute('SELECT name FROM pet_profile WHERE id=?',(A,)).fetchone()[0]=='其他視窗新值'
    assert db.execute('SELECT count(*) FROM fixture_settings_writes').fetchone()[0]==writes
    record('PET-65-01-過期草稿拒絕、保留輸入、新值與零寫入')
    click('重新讀取設定');wait(lambda:evaluate("return document.querySelector('[aria-labelledby=settings-reload-title]')!==null;"))
    click('捨棄並重新讀取');wait(lambda:evaluate("return document.querySelector('[name=settings-name]').value==='其他視窗新值';"))
    edit('驗收甲');click('儲存設定');wait(lambda:db.execute('SELECT name FROM pet_profile WHERE id=?',(A,)).fetchone()[0]=='驗收甲');before=snapshot()
    record('PET-65-02-明確捨棄重讀後可正常儲存')
    edit('甲未存草稿');
    choose(B);
    wait(dialog)
    assert selected()==A;assert evaluate("return document.querySelector('[name=settings-name]').value;")=='甲未存草稿';assert snapshot()==before
    record('PET-63-01-換ID先確認且原ID草稿與DB保留')
    click('留在此頁');
    wait(lambda:not dialog());
    assert selected()==A;
    assert snapshot()==before
    record('PET-63-02-留頁不存檔不切換')
    choose(B);
    wait(dialog);
    click('捨棄並切換');
    wait(lambda:selected()==B and evaluate("return document.querySelector('[name=settings-name]').value==='驗收乙';"));assert snapshot()==before
    record('PET-63-03-捨棄後載入另一ID且不改活動')
    edit('乙已存名稱');
    choose(A);
    wait(dialog);
    click('儲存後切換');
    wait(lambda:selected()==A and evaluate("return document.querySelector('[name=settings-name]').value==='驗收甲';"))
    assert db.execute('SELECT name FROM pet_profile WHERE id=?',(B,)).fetchone()[0]=='乙已存名稱';
    assert snapshot()['activity']==before['activity']
    record('PET-63-04-儲存舊ID成功後才切換且不影響活動')
    edit('甲失敗草稿');evaluate("document.querySelector('#pet-memory-tab').click();");wait(dialog)
    evaluate("window.originalSavePet63=window.savePetSettingsChecked;window.savePetSettingsChecked=()=>Promise.reject('合成儲存故障');")
    click('儲存後切換');wait(lambda:evaluate("return document.querySelector('#pet-settings').textContent.includes('儲存失敗');"));assert dialog();assert evaluate("return !document.querySelector('#pet-settings').hidden;");assert db.execute('SELECT name FROM pet_profile WHERE id=?',(A,)).fetchone()[0]=='驗收甲'
    record('PET-63-05-儲存失敗保留原頁草稿與待切目標')
    reqshot=req('GET',f'/session/{sid}/screenshot');(O/(phase+'-pet.png')).write_bytes(base64.b64decode(reqshot))
    evaluate("window.savePetSettingsChecked=window.originalSavePet63;");click('儲存後切換');wait(lambda:evaluate("return !document.querySelector('#pet-memory').hidden;"));assert db.execute('SELECT name FROM pet_profile WHERE id=?',(A,)).fetchone()[0]=='甲失敗草稿';assert not dialog()
    record('PET-63-06-重試成功後才切頁')
    evaluate("document.querySelector('#pet-settings-tab').click();");edit('甲捨棄草稿');evaluate("document.querySelector('#pet-files-tab').click();");wait(dialog);click('捨棄並切換');wait(lambda:evaluate("return !document.querySelector('#pet-files').hidden;"))
    evaluate("document.querySelector('#pet-settings-tab').click();");wait(lambda:evaluate("return document.querySelector('[name=settings-name]').value==='甲失敗草稿';"));assert snapshot()['activity']==before['activity']
    record('PET-63-07-捨棄切頁後重返設定為已存值且生命週期不變')
    req('DELETE',f'/session/{sid}');sid=None
    result=req('POST','/session',{'capabilities':{'alwaysMatch':{'browserName':'wry','tauri:options':{'application':binary}}}});sid=result['sessionId'];wait(lambda:evaluate("return !!document.querySelector('#pet-settings-tab');"));evaluate("document.querySelector('#pet-settings-tab').click();");choose(B);wait(lambda:evaluate("return document.querySelector('[name=settings-name]')?.value==='乙已存名稱';"))
    assert snapshot()['activity']==before['activity'];record('PET-63-08-程序重啟仍可依ID讀回已存設定')
    assert evaluate("return Array.from(document.querySelectorAll('[role=tab]')).map(n=>n.textContent.trim());")==['互動','設定','記憶','檔案','管理','更新']
    assert evaluate("return document.querySelectorAll('.pet-settings-form button[type=submit]').length;")==1
    evaluate("window.scrollTo(0,0);")
    (O/(phase+'-settings-clean.png')).write_bytes(base64.b64decode(req('GET',f'/session/{sid}/screenshot')))
    evaluate("document.querySelector('#pet-interaction-tab').click();window.scrollTo(0,0);")
    wait(lambda:evaluate("return document.querySelector('#pet-interaction-tab').getAttribute('aria-selected')==='true';"))
    (O/(phase+'-interaction.png')).write_bytes(base64.b64decode(req('GET',f'/session/{sid}/screenshot')))
    assert snapshot()['activity']==before['activity']
    record('PET-UI-13-六分頁與直接儲存入口、互動頁切換零活動寫入')
    evaluate("document.querySelector('#pet-files-tab').click();")
    wait(lambda:evaluate("return document.querySelector('.file-list')?.textContent.includes('子資料夾');"))
 
    evaluate("window.day66Browse=window.browseAuthorizedDirectory;window.day66Calls=[];window.day66Fail=true;window.browseAuthorizedDirectory=async(path)=>{window.day66Calls.push(path);if(window.day66Fail&&JSON.stringify(path)===JSON.stringify(['子資料夾'])){window.day66Fail=false;throw '目錄讀取暫時失敗（契約注入）';}return window.day66Browse(path);};")
    before_files=snapshot()
    click('子資料夾');wait(lambda:evaluate("return document.body.textContent.includes('目錄讀取暫時失敗');"))
    click('重試讀取');wait(lambda:evaluate("return document.querySelector('.file-list')?.textContent.includes('實際內容.txt');"))
    assert evaluate("return window.day66Calls;")==[['子資料夾'],['子資料夾']]
    assert snapshot()==before_files
    (O/(phase+'-file-retry.png')).write_bytes(base64.b64decode(req('GET',f'/session/{sid}/screenshot')))
    (O/(phase+'-file-retry-calls.json')).write_text(json.dumps(evaluate("return window.day66Calls;"),ensure_ascii=False))
    record('PET-66-失敗目標精確重試後經真IPC取得檔案且零資料寫入')
    baseline_files=snapshot()
    evaluate("const n=document.querySelector('#file-name-filter');if(!n)throw Error('缺少目錄名稱篩選');n.value='不存在';n.dispatchEvent(new Event('input',{bubbles:true}));")
    wait(lambda:evaluate("return document.querySelector('.file-browser').textContent.includes('沒有符合名稱的項目');"))
    assert evaluate("return document.querySelectorAll('.file-list tbody input[type=checkbox]').length;")==0
    evaluate("const n=document.querySelector('#file-name-filter');n.value='內容';n.dispatchEvent(new Event('input',{bubbles:true}));")
    wait(lambda:evaluate("return document.querySelector('.file-list').textContent.includes('實際內容.txt');"))
    click('清除名稱篩選');assert evaluate("return document.querySelector('#file-name-filter').value;")==''
    assert snapshot()==baseline_files;assert evaluate("return window.day66Calls;")==[['子資料夾'],['子資料夾']]
    (O/(phase+'-file-name-filter.png')).write_bytes(base64.b64decode(req('GET',f'/session/{sid}/screenshot')))
    record('PET-67-目前目錄即時名稱篩選／無符合／清除且零額外IPC及DB寫入')


    # 管理資料表使用獨立SQLite內的真實測試寵物。
    for i in range(27):
        pet_id=str(uuid.uuid4());db.execute("INSERT INTO pet_profile(id,name,species,owner_id,is_default_closure) VALUES(?,?,'tortoise','local-owner',1)",(pet_id,f'表格測試{i:02}'))
    for kind in ['walk','sit']:
        db.execute("INSERT INTO pet_activity(id,pet_id,owner_id,activity_type,is_activity,is_use_ai) VALUES(?,?,'local-owner',?,0,0)",(str(uuid.uuid4()),pet_id,kind))
        db.commit();table_before=snapshot();slot_before=db.execute('SELECT * FROM local_pet_selection').fetchall()
    
    evaluate("document.querySelector('#pet-management-tab').click();")
    req('POST',f'/session/{sid}/window/rect',{'width':1280,'height':900})
    click('重新載入清單')
    wait(lambda:evaluate("return document.querySelectorAll('.management-data-table tbody tr[data-pet-id]').length===10;"))
    assert evaluate("return Array.from(document.querySelectorAll('.management-data-table thead th')).map(n=>n.textContent.trim());")==['名稱','種類','識別碼','操作']
    assert evaluate("const q=document.querySelector('#management-table-search').getBoundingClientRect(),b=document.querySelector('.management-query-row > button').getBoundingClientRect();return Math.abs(q.y-b.y)<2&&Math.abs(q.height-b.height)<2;")
 
    def table_click(label):
        evaluate("const b=Array.from(document.querySelectorAll('#pet-management button')).find(n=>n.textContent.trim()===arguments[0]);if(b)b.scrollIntoView({block:'nearest',inline:'nearest'});",label)
        el=req('POST',f'/session/{sid}/element',{'using':'xpath','value':"//*[@id='pet-management']//button[normalize-space(.)='"+label+"']"});
        req('POST',f'/session/{sid}/element/'+el['element-6066-11e4-a52e-4f735466cecf']+'/click',{})
    def filter_text(text):
        evaluate("const n=document.querySelector('#management-table-search');n.value=arguments[0];n.dispatchEvent(new Event('input',{bubbles:true}));",text)
    def table_size(n):
        evaluate("const n=document.querySelector('#management-table-size');n.value=arguments[0];n.dispatchEvent(new Event('change',{bubbles:true}));",str(n))
        
    table_click('下一頁');
    wait(lambda:evaluate("return document.querySelector('.management-table-pagination').textContent.includes('第 2 / 3 頁');"))
    table_size(25);
    wait(lambda:evaluate("return document.querySelectorAll('.management-data-table tbody tr[data-pet-id]').length===25;"))
    table_size(50);
    wait(lambda:evaluate("return document.querySelectorAll('.management-data-table tbody tr[data-pet-id]').length===29;"))
    table_size(10);
    table_click('名稱');
    wait(lambda:evaluate("return document.querySelector('.management-data-table th').getAttribute('aria-sort')==='descending';"))
    assert evaluate("return document.querySelector('.management-pet-name').textContent;")=='表格測試26'
    filter_text('表格測試2');
    wait(lambda:evaluate("return document.querySelectorAll('.management-data-table tbody tr[data-pet-id]').length===7;"))
    filter_text('TORToise');
    wait(lambda:evaluate("return document.querySelector('.management-table-info').textContent.includes('共 29 筆符合');"))
    filter_text('不存在的寵物');
    wait(lambda:evaluate("return document.querySelector('.management-data-table tbody').textContent.includes('沒有符合的寵物');"))
    table_click('清除搜尋');
    assert evaluate("return document.querySelector('#management-table-search').value;")==''
    assert snapshot()==table_before and db.execute('SELECT * FROM local_pet_selection').fetchall()==slot_before
    record('PET-TABLE-01-真29隻清單、10／25／50分頁、排序與中文模糊搜尋零DB寫入')
    # 管理表僅新增／刪除；互動與設定沿同一UUID選取器。
    assert evaluate("return document.querySelectorAll('#pet-management .management-row-actions').length===0;")
    assert evaluate("return !document.querySelector('#pet-management [aria-label=\"寵物新名稱\"]');")
    assert evaluate("return Array.from(document.querySelectorAll('#pet-management tbody button')).every(n=>n.textContent.trim()==='刪除'&&n.disabled);")
    record('PET-68-管理DataTable僅保留新增與未接通刪除，移除更名及生活窗操作')
    evaluate("document.querySelector('#pet-interaction-tab').click();");
    
    choose(A)
    wait(lambda:evaluate("return document.querySelector('.control-pet-picker select').value===arguments[0];",A))
    assert snapshot()==table_before and db.execute('SELECT * FROM local_pet_selection').fetchall()==slot_before
    record('PET-TABLE-02-互動選取只切換對象，不出籠或寫入資料')
    evaluate("document.querySelector('#pet-settings-tab').click();");wait(lambda:evaluate("return document.querySelector('[name=settings-name]')?.value==='甲失敗草稿';"));edit('管理設定更名');click('儲存設定')
    wait(lambda:db.execute('SELECT name FROM pet_profile WHERE id=?',(A,)).fetchone()[0]=='管理設定更名')
    assert snapshot()['activity']==table_before['activity'];assert db.execute('SELECT name FROM pet_profile WHERE id=?',(B,)).fetchone()[0]=='乙已存名稱'
    evaluate("document.querySelector('#pet-management-tab').click();")
    record('PET-TABLE-03-名稱由設定真IPC更新指定ID，不改他寵或活動')
    filter_text('');evaluate("window.scrollTo(0,0);")
    req('POST',f'/session/{sid}/window/rect',{'width':1280,'height':900});time.sleep(.3)
    (O/(phase+'-management-desktop.png')).write_bytes(base64.b64decode(req('GET',f'/session/{sid}/screenshot')))
    req('POST',f'/session/{sid}/window/rect',{'width':420,'height':820});time.sleep(.3)
    assert evaluate("return document.documentElement.scrollWidth<=window.innerWidth;")
    assert evaluate("const q=document.querySelector('#management-table-search').getBoundingClientRect(),b=document.querySelector('.management-query-row > button').getBoundingClientRect();return Math.abs(q.y-b.y)<2&&b.right<=innerWidth;")
    assert evaluate("const n=document.querySelector('.management-table-scroll');return n.scrollWidth>n.clientWidth;")
    (O/(phase+'-viewport.json')).write_text(json.dumps(evaluate("return {width:innerWidth,height:innerHeight};")))
    evaluate("document.querySelector('.management-table-toolbar').scrollIntoView({block:'start'});")
    (O/(phase+'-management-mobile.png')).write_bytes(base64.b64decode(req('GET',f'/session/{sid}/screenshot')))
    record('PET-TABLE-04-桌面／窄窗表格獨立捲動，整頁無水平溢出')
    req('POST',f'/session/{sid}/window/rect',{'width':1280,'height':900});evaluate("document.querySelector('#pet-interaction-tab').click();");choose(A)
    wait(lambda:evaluate("return document.querySelector('.pet-workspace .control-primary')?.textContent.trim()==='出籠';"))
    evaluate("document.querySelector('.pet-workspace .control-primary').click();")
    wait(lambda:db.execute('SELECT count(*) FROM pet_activity WHERE pet_id=? AND is_activity=1',(A,)).fetchone()[0]>0)
    wait(lambda:evaluate("return document.querySelector('.pet-workspace .pet-status-chip').textContent.includes('已出籠');"))
    click('更新活動');wait(lambda:evaluate("return !document.querySelector('.pet-workspace-bar [data-action=refresh]').disabled;"))
    click('找回生活窗');wait(lambda:evaluate("return !document.querySelector('.pet-workspace .control-primary').disabled;"))
    evaluate("document.querySelector('.pet-workspace .control-primary').click();")
    wait(lambda:db.execute('SELECT count(*) FROM pet_activity WHERE pet_id=? AND is_activity=1',(A,)).fetchone()[0]==0)
    assert db.execute('SELECT count(*) FROM pet_activity WHERE pet_id=? AND is_activity=1',(B,)).fetchone()[0]==0
    record('PET-TABLE-05-互動區出籠／狀態／找回／回籠真IPC保持逐ID隔離')
    (O/(phase+'-interaction-actions.png')).write_bytes(base64.b64decode(req('GET',f'/session/{sid}/screenshot')))
    record('PET-68-原管理生活窗操作整合互動既有控制，不重複提供更名')
    audit=evaluate("return Array.from(document.querySelectorAll('button')).map(n=>({text:n.textContent.trim(),area:n.closest('[role=tabpanel]')?.id??(n.getAttribute('role')==='tab'?'導覽':'共用'),group:n.closest('[role=group]')?.getAttribute('aria-label')??n.closest('fieldset')?.querySelector('legend')?.textContent??'',disabled:n.disabled,visible:!!(n.offsetWidth||n.offsetHeight||n.getClientRects().length)}));")
    (O/(phase+'-button-inventory.json')).write_text(json.dumps(audit,ensure_ascii=False,indent=2))
    assert evaluate("return document.querySelectorAll('#pet-interaction .pet-preview,#pet-interaction .pet-controls').length===0;")
    assert evaluate("return document.querySelector('[role=group][aria-label=\"生活窗操作\"]')!==null&&document.querySelector('[role=group][aria-label=\"活動模式\"]')!==null;")
    record('PET-68-六區按鈕逐項盤點、生活窗與活動分組、聊天集中且無重複手動回應／預覽')
    
except Exception:
   import traceback
   rows.append({'name':'NATIVE_REGRESSION','status':'FAIL','error':traceback.format_exc()});
   print(traceback.format_exc(),flush=True)
finally:
    if sid:
        try:
            req('DELETE',f'/session/{sid}')
        except Exception:
            pass
    if driver:
        try:
            os.killpg(driver.pid,signal.SIGTERM);
            driver.wait(timeout=10)
        except Exception:
            pass
    if wm:
        wm.terminate();
        wm.wait(timeout=10)
    (O/(phase+'-native-results.json')).write_text(json.dumps({'phase':phase,'database':str(data),'cases':rows,'before':before,'after':snapshot()},ensure_ascii=False,indent=2));db.close()
    if any(r['status']=='FAIL' for r in rows):raise SystemExit(1)