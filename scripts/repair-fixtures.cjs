// 只在隔离验收实例生成配置，随后由 Test-Repair.ps1 制作导入文件。
async (page) => {
  const invoke=(cmd,args={})=>page.evaluate(([cmd,args])=>window.__TAURI_INTERNALS__.invoke(cmd,args),[cmd,args]);
  const app=await invoke('get_app_info');
  if(!/[\\/]test-results[\\/]/.test(app.dataDir))throw new Error('只允许隔离验收目录');
  const template=(await invoke('list_presets'))[0];
  await invoke('save_preset',{preset:{...template,id:'',name:'修复验收图标',icon:'🚀',groupId:'',kind:'powershell',program:'Write-Output REPAIR_COPY_OK',args:[],env:[],workingDir:app.dataDir,runtime:{kind:'',path:'',managerPath:''},confirm:false,elevated:false,placeholderArgs:[]}});
  await invoke('export_data',{path:app.dataDir+'\\repair-export.json'});
  return {dataDir:app.dataDir};
}
