// 必须在结束并重新启动应用后执行，证明恢复布局不会启动命令。
async (page) => {
  await page.locator('.cd-workspace__pane').first().waitFor({state:'attached'});
  const result=await page.evaluate(async()=>{
    const invoke=(cmd)=>window.__TAURI_INTERNALS__.invoke(cmd);
    const app=await invoke('get_app_info');
    if(!/[\\/]test-results[\\/]/.test(app.dataDir))throw new Error('只允许隔离数据目录');
    const config=await invoke('get_productivity'),sessions=await invoke('list_terminal_sessions');
    const restored=(document.body.textContent||'').includes('已恢复布局，命令尚未执行');
    if(sessions.length!==0||!restored)throw new Error('布局恢复不应创建后端任务');
    return {version:app.version,backendSessions:sessions.length,savedTabs:config.layouts[config.activeWorkspace]?.tabs.length,restored};
  });
  await page.screenshot({path:'output/playwright/v12-restored.png'});
  return result;
}
