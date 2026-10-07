// 检查两个真实窗口的设置同步、关闭副窗口保留任务，以及系统拒绝时的回滚。
async (page) => {
  const invoke=(target,cmd,args={})=>target.evaluate(([cmd,args])=>window.__TAURI_INTERNALS__.invoke(cmd,args),[cmd,args]);
  const app=await invoke(page,'get_app_info');
  if(!/[\\/]test-results[\\/]/.test(app.dataDir))throw new Error('只允许隔离验收目录');
  const settings=await invoke(page,'get_settings');
  let secondary=page.context().pages().find(p=>p!==page&&p.url()===page.url());
  if(!secondary){await invoke(page,'create_work_window');await page.waitForTimeout(1200);secondary=page.context().pages().find(p=>p!==page&&p.url()===page.url());}
  if(!secondary)throw new Error('副窗口未创建');
  await invoke(page,'save_settings',{settings:{...settings,theme:'light'}});
  await secondary.waitForFunction(()=>document.documentElement.dataset.theme==='light');
  await invoke(secondary,'save_settings',{settings:{...settings,theme:'dark'}});
  await page.waitForFunction(()=>document.documentElement.dataset.theme==='dark');
  const task=await invoke(page,'open_shell',{kind:'cmd',workspaceId:''});
  await secondary.evaluate(()=>window.__TAURI_INTERNALS__.invoke('plugin:window|close')).catch(e=>{if(!String(e).includes('closed'))throw e;});
  const alive=(await invoke(page,'list_terminal_sessions')).find(s=>s.sessionId===task.sessionId);
  if(!alive||alive.status!=='running')throw new Error('副窗口关闭影响了后端任务');
  await invoke(page,'kill_terminal',{sessionId:task.sessionId});await invoke(page,'wait_terminal_exit',{sessionId:task.sessionId,timeoutMs:10000});
  const autostart=await page.evaluate(async()=>{try{await window.__TAURI_INTERNALS__.invoke('set_autostart',{enabled:true});return {ok:true};}catch(e){return {ok:false,error:e};}});
  const after=await invoke(page,'get_settings');
  if(!autostart.ok&&after.autostart!==settings.autostart)throw new Error('自启注册失败但设置却被修改');
  await invoke(page,'set_autostart',{enabled:settings.autostart}).catch(()=>{});
  await invoke(page,'save_settings',{settings});
  return {version:app.version,multiWindowSync:true,closeKeepsTask:true,autostart};
}
