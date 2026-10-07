// Playwright CLI 验收：只连接 test-results 下的隔离数据目录。
async (page) => {
  const report = await page.evaluate(async () => {
    const invoke = async (cmd,args={}) => {
      try {return await window.__TAURI_INTERNALS__.invoke(cmd,args);}
      catch(e){throw new Error(`${cmd}: ${JSON.stringify(e)}`);}
    };
    const app = await invoke('get_app_info');
    if(!/[\\/]test-results[\\/]/.test(app.dataDir))throw new Error('只允许使用隔离验收目录');
    const checks=[];
    const check=(name,ok,detail='')=>{checks.push({name,ok,detail});if(!ok)throw new Error(`${name}: ${detail}`);};
    const template=(await invoke('list_presets'))[0];
    const make=(name,kind,program,args=[],extra={})=>invoke('save_preset',{preset:{...template,id:'',name:`V12 ${name}`,kind,program,args,workingDir:'',env:[],runtime:{kind:'',path:'',managerPath:''},groupId:'',useShell:kind==='powershell',confirm:false,elevated:false,placeholderArgs:[],...extra}});
    const original=await invoke('get_productivity');
    const envs=await invoke('discover_environments',{projectDir:app.dataDir});
    const node=envs.environments.find(e=>e.kind==='node');
    const python=envs.environments.find(e=>e.kind==='python');
    check('识别可用 Python / Node',!!node&&!!python);
    const p=await make('工作区继承','powershell','Write-Output (Get-Location).Path; Write-Output $env:V12_WORKSPACE');
    const workspace={id:crypto.randomUUID(),name:'验收项目',directory:app.dataDir,python:{kind:python.kind,path:python.path,managerPath:python.managerPath},node:{kind:node.kind,path:node.path,managerPath:node.managerPath},env:[{name:'V12_WORKSPACE',value:'WORKSPACE_OK'}],presetIds:[p.id],ports:[]};
    await invoke('save_productivity',{config:{...original,workspaces:[workspace],activeWorkspace:workspace.id}});
    check('SQLite 保存项目',(await invoke('get_productivity')).workspaces[0].id===workspace.id);
    const flight=await invoke('preflight_preset',{preset:p,args:{},workspaceId:workspace.id});
    check('项目运行目录检查',flight.canRun&&flight.directory===app.dataDir,JSON.stringify(flight));
    const badDir=await invoke('preflight_preset',{preset:{...p,workingDir:app.dataDir+'\\does-not-exist'},args:{},workspaceId:workspace.id});
    check('拦截不存在的目录',!badDir.canRun);
    const missing=await invoke('preflight_preset',{preset:{...p,program:'Write-Output {{missing}}'},args:{},workspaceId:workspace.id});
    check('检测未填参数',!missing.canRun);
    const badExe=await invoke('preflight_preset',{preset:{...p,kind:'node',program:'cmddeck-no-such-program.exe',useShell:false},args:{},workspaceId:''});
    check('检测缺失程序',!badExe.canRun);
    const run=await invoke('run_preset',{presetId:p.id,args:{},confirmed:false,workspaceId:workspace.id});
    const code=await invoke('wait_terminal_exit',{sessionId:run.sessionId,timeoutMs:15000});
    const text=await invoke('get_terminal_snapshot',{sessionId:run.sessionId});
    check('项目目录和环境变量实际生效',code===0&&text.includes(app.dataDir)&&text.includes('WORKSPACE_OK'),text);
    const history=(await invoke('list_log_entries')).find(h=>h.sessionId===run.sessionId);
    check('历史目录只传元数据',!!history&&history.outputTail==='');
    check('关闭后仍可读取日志',(await invoke('get_history_output',{id:history.id})).includes('WORKSPACE_OK'));
    await invoke('export_task_log',{sessionId:run.sessionId,historyId:null,path:app.dataDir+'\\task.log'});
    check('导出日志',true);
    const service=await make('端口服务','node','node.exe',['-e',"const s=require('http').createServer((q,r)=>r.end('V12_OK'));s.listen(0,'127.0.0.1',()=>console.log('V12_PORT:'+s.address().port));"],{useShell:false});
    const running=await invoke('run_preset',{presetId:service.id,args:{},confirmed:false,workspaceId:workspace.id});
    let port=0;
    for(let i=0;i<50;i++){const output=await invoke('get_terminal_snapshot',{sessionId:running.sessionId});const match=output.match(/V12_PORT:(\d+)/);if(match){port=Number(match[1]);break;}await new Promise(r=>setTimeout(r,100));}
    const tasks=await invoke('list_task_metrics');
    const task=tasks.find(t=>t.terminal.sessionId===running.sessionId);
    check('任务进程树与内存采样',task.pids.length>0&&task.memoryBytes>0,JSON.stringify(task));
    const ports=await invoke('list_listening_ports');
    check('真实 TCP 端口归属',ports.some(p=>p.port===port&&p.sessionId===running.sessionId),JSON.stringify({port,ports:ports.filter(p=>p.port===port)}));
    await invoke('save_productivity',{config:{...(await invoke('get_productivity')),workspaces:[{...workspace,ports:[port]}]}});
    const occupied=await invoke('preflight_preset',{preset:p,args:{},workspaceId:workspace.id});
    check('端口占用提示',occupied.checks.some(c=>c.level==='warning'&&c.message.includes(String(port))));
    const options={presetIds:[p.id],includeEnvironment:false,includePaths:false,includeSettings:false};
    const exported=JSON.parse(await invoke('preview_config_export',{options}));
    check('分享默认排除私人字段',exported.presets.length===1&&exported.presets[0].env.length===0&&!exported.presets[0].workingDir&&!exported.presets[0].runtime.path&&exported.productivity.workspaces.length===0);
    const layout={mode:'columns',tabs:[{title:'已保存终端',kind:p.kind,cwd:app.dataDir,presetId:p.id}],active:0,secondary:0};
    await invoke('save_terminal_layout',{workspaceId:workspace.id,layout});
    check('保存终端布局',(await invoke('get_productivity')).layouts[workspace.id].mode==='columns');
    const backup=await invoke('create_config_backup');
    await invoke('delete_preset',{id:p.id});
    let blocked=false;try{await invoke('restore_config_backup',{id:backup,confirmed:true});}catch(e){blocked=e.message.includes('停止');}
    check('运行期间阻止恢复',blocked);
    await invoke('kill_terminal',{sessionId:running.sessionId});
    await invoke('wait_terminal_exit',{sessionId:running.sessionId,timeoutMs:10000});
    await invoke('restore_config_backup',{id:backup,confirmed:true});
    check('恢复被删预设',(await invoke('list_presets')).some(x=>x.id===p.id));
    check('恢复项目与布局',(await invoke('get_productivity')).layouts[workspace.id].mode==='columns');
    let traversal=false;try{await invoke('restore_config_backup',{id:'../../outside.json',confirmed:true});}catch{traversal=true;}
    check('拒绝备份路径穿越',traversal);
    check('自动备份保留上限',(await invoke('list_config_backups')).length<=30);
    let update;try{update=await invoke('check_for_updates');check('在线更新检查',!!update.latest&&typeof update.available==='boolean',JSON.stringify(update));}catch(e){checks.push({name:'在线更新检查',ok:false,networkLimited:true,detail:e.message});}
    return {checks,workspaceId:workspace.id,presetId:p.id,nodePresetId:service.id,update};
  });
  return report;
}
