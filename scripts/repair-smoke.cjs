// 1.2.1 回归，仅允许操作 test-results 下的隔离实例。
async (page) => {
  const invoke=(cmd,args={})=>page.evaluate(([cmd,args])=>window.__TAURI_INTERNALS__.invoke(cmd,args),[cmd,args]);
  const app=await invoke('get_app_info');
  if(!/[\\/]test-results[\\/]/.test(app.dataDir))throw new Error('只允许隔离验收目录');
  const checks=[],check=(name,ok,detail='')=>{checks.push({name,ok,detail});if(!ok)throw new Error(name+': '+detail);};
  const original=await invoke('get_settings');
  await invoke('save_settings',{settings:{...original,allowInteractiveInput:false,allowedExecutables:[],firstRunDone:true}});
  const template=(await invoke('list_presets'))[0];
  const make=(extra={})=>invoke('save_preset',{preset:{...template,id:'',name:'修复验收图标',icon:'🚀',groupId:'',kind:'powershell',program:'Write-Output REPAIR_COPY_OK',args:[],env:[],workingDir:app.dataDir,runtime:{kind:'',path:'',managerPath:''},confirm:false,elevated:false,placeholderArgs:[],...extra}});
  const p=(await invoke('list_presets')).filter(p=>p.name==='修复验收图标').at(-1);
  if(!p)throw new Error('请先运行 Test-Repair.ps1 生成隔离测试数据');
  const before=await invoke('list_presets');
  const invalidPath=app.dataDir+'\\repair-invalid.json';
  const importError=await page.evaluate(async(path)=>{try{await window.__TAURI_INTERNALS__.invoke('import_data',{path,mode:'replace'});return null;}catch(e){return e;}},invalidPath);
  const rejected=String(importError?.message||JSON.stringify(importError)).includes('100');
  const after=await invoke('list_presets');
  check('覆盖导入坏记录拒绝且原预设完全保留',rejected&&JSON.stringify(before)===JSON.stringify(after),JSON.stringify({importError,unchanged:JSON.stringify(before)===JSON.stringify(after)}));
  const goodPath=app.dataDir+'\\repair-valid.json';
  await invoke('import_data',{path:goodPath,mode:'replace'});
  check('有效覆盖导入完整提交',(await invoke('list_presets')).length===1&&(await invoke('get_preset',{id:p.id})).notes==='事务导入成功');
  await page.getByRole('button',{name:'⌘ 预设指令',exact:true}).click();
  await page.locator('.preset-card [data-icon="🚀"]').waitFor();
  check('自定义图标显示在真实卡片',await page.locator('.preset-card [data-icon="🚀"]').isVisible());
  const shell=await invoke('open_shell',{kind:'cmd',workspaceId:''});
  let inputRejected=0;for(const data of ['e','cho MUST_NOT_RUN\r','\x1b[200~echo MUST_NOT_RUN\x1b[201~']){try{await invoke('write_terminal',{sessionId:shell.sessionId,data});}catch{inputRejected++;}}
  check('只读拦截逐字输入和粘贴',inputRejected===3);
  await invoke('write_terminal',{sessionId:shell.sessionId,data:'\x1b[1;1R'});
  check('只读仍允许 ConPTY 协议回应',true);
  await invoke('kill_terminal',{sessionId:shell.sessionId});await invoke('wait_terminal_exit',{sessionId:shell.sessionId,timeoutMs:10000});
  const settings=await invoke('get_settings'),cmd=(await invoke('get_system_shells')).find(s=>s.id==='cmd');
  await invoke('save_settings',{settings:{...settings,allowedExecutables:[cmd.path]}});
  const denied=await page.evaluate(async(id)=>{try{await window.__TAURI_INTERNALS__.invoke('run_preset',{presetId:id,args:{},confirmed:false});return false;}catch(e){return String(e.message||JSON.stringify(e)).includes('白名单');}},p.id);
  check('执行白名单覆盖解释器类型',denied);
  const cp=await make({name:'修复验收白名单',kind:'cmd',program:'echo ALLOWLIST_OK'});
  const cr=await invoke('run_preset',{presetId:cp.id,args:{},confirmed:false});
  check('白名单允许精确程序路径',(await invoke('wait_terminal_exit',{sessionId:cr.sessionId,timeoutMs:15000}))===0);
  await invoke('save_settings',{settings});
  const admin=await make({name:'修复验收管理员',elevated:true});
  const flight=await invoke('preflight_preset',{preset:admin,args:{}});
  check('管理员预设预检查允许 UAC 流程',flight.canRun);
  let confirmation=false;try{await invoke('run_preset',{presetId:admin.id,args:{},confirmed:false});}catch{confirmation=true;}
  check('管理员任务不能未经确认启动',confirmation);
  const run=await invoke('run_preset',{presetId:p.id,args:{},confirmed:false});await invoke('wait_terminal_exit',{sessionId:run.sessionId,timeoutMs:15000});
  await page.getByRole('button',{name:'⌘ 预设指令',exact:true}).click();
  await page.waitForTimeout(300);
  await page.getByTitle('复制全部输出到剪贴板',{exact:true}).click();
  await page.getByText('已复制全部输出',{exact:true}).last().waitFor();
  const copied=await invoke('plugin:clipboard-manager|read_text');
  check('系统剪贴板实际包含终端纯文本',copied.includes('REPAIR_COPY_OK')&&!copied.includes('\x1b'));
  await page.getByTitle('清空当前屏幕（Ctrl+L 同样有效）',{exact:true}).click();
  await page.getByTitle('复制全部输出到剪贴板',{exact:true}).click();
  await page.getByText('没有可复制的内容',{exact:true}).last().waitFor();
  check('清屏后终端渲染缓冲为空',true);
  const integration=await invoke('get_integration_status');
  check('托盘调度器与快捷键已初始化',integration.tray&&integration.scheduler&&!!integration.hotkey);
  return {version:app.version,checks};
}
