// 通过 Playwright CLI 的 run-code --filename 执行；连接真实 Tauri 窗口。
// 验收使用独立 CMDDECK_DATA_DIR，不改动用户的正式配置。
async (page) => {
  const result = await page.evaluate(async () => {
    const invoke = async (cmd, args) => {
      try {
        return await window.__TAURI_INTERNALS__.invoke(cmd, args);
      } catch (e) {
        throw new Error(cmd + ": " + JSON.stringify(e));
      }
    };
    const report = { at: new Date().toISOString(), checks: [], sessions: [] };
    const check = (name, ok, detail) => {
      report.checks.push({ name, ok, detail });
      if (!ok) throw new Error(name + ": " + detail);
    };
    const appInfo = await invoke("get_app_info");
    if (!/[\\/]test-results[\\/]/.test(appInfo.dataDir)) throw new Error("验收只允许连接 test-results 下的独立数据目录");
    const original = await invoke("get_settings");
    const settings = { ...original, allowUnknownExe: true };
    await invoke("save_settings", { settings });
    const template = (await invoke("list_presets"))[0];
    const make = async (name, kind, program, args = [], extra = {}) =>
      invoke("save_preset", {
        preset: {
          ...template,
          id: "",
          name: "验收 " + name,
          kind,
          program,
          args,
          useShell: ["cmd", "powershell"].includes(kind),
          confirm: false,
          elevated: false,
          favorite: false,
          workingDir: "",
          env: [],
          groupId: "",
          placeholderArgs: [],
          ...extra,
        },
      });
    const run = async (p, args = {}, expected = "") => {
      const info = await invoke("run_preset", {
        presetId: p.id,
        args,
        confirmed: false,
      });
      const code = await invoke("wait_terminal_exit", {
        sessionId: info.sessionId,
        timeoutMs: 15000,
      });
      const text = await invoke("get_terminal_snapshot", {
        sessionId: info.sessionId,
      });
      report.sessions.push({
        name: p.name,
        id: info.sessionId,
        code,
        output: text,
      });
      check(
        p.name,
        code === 0 && text.includes(expected),
        `code=${code}, contains=${expected}`,
      );
      return info;
    };
    const cmd = await make("CMD", "cmd", "echo CMD_OK");
    const ps = await make(
      "PowerShell",
      "powershell",
      'Write-Output "中文 {{name}}"; Write-Output $env:DECK_TEST',
      [],
      { env: [{ name: "DECK_TEST", value: "ENV_OK" }] },
    );
    const py = await make("Python", "python", "python.exe", [
      "-c",
      'print("PY_OK 中文")',
    ]);
    const node = await make("Node", "node", "node.exe", [
      "-e",
      'console.log("NODE_OK")',
    ]);
    const exe = await make(
      "自定义 exe",
      "exe",
      "C:\\Windows\\System32\\where.exe",
      ["cmd.exe"],
    );
    await run(cmd, {}, "CMD_OK");
    await run(ps, { name: "占位符成功" }, "占位符成功");
    await run(py, {}, "PY_OK");
    await run(node, {}, "NODE_OK");
    await run(exe, {}, "cmd.exe");
    const firstPs = report.sessions.find((s) => s.name === ps.name);
    check("环境变量", firstPs.output.includes("ENV_OK"), firstPs.output);
    const updated = await invoke("save_preset", {
      preset: { ...cmd, name: "验收 CMD 已编辑", favorite: true },
    });
    const saved = await invoke("get_preset", { id: cmd.id });
    check(
      "编辑与收藏保存",
      saved.name === updated.name && saved.favorite,
      saved.name,
    );
    const denied = await make("黑名单", "cmd", "diskpart");
    let blocked = false;
    try {
      await invoke("run_preset", {
        presetId: denied.id,
        args: {},
        confirmed: true,
      });
    } catch (e) {
      blocked = String(e).includes("黑名单");
    }
    check("黑名单后端拦截", blocked, "diskpart 必须在创建进程之前阻止");
    const dangerous = await make("二次确认", "cmd", "echo CONFIRM_OK", [], {
      confirm: true,
    });
    let refused = false;
    try {
      await invoke("run_preset", { presetId: dangerous.id, args: {} });
    } catch (e) {
      refused = true;
    }
    check("未确认预设不能运行", refused, "confirm=true");
    const long = await make(
      "停止与并发",
      "powershell",
      "Write-Output START; Start-Sleep -Seconds 20; Write-Output END",
    );
    const a = await invoke("run_preset", { presetId: long.id, args: {} }),
      b = await invoke("run_preset", { presetId: long.id, args: {} });
    const live = await invoke("list_terminal_sessions");
    check(
      "多任务并发",
      live.filter((s) => s.status === "running").length >= 2,
      "两个独立 ConPTY 会话",
    );
    await invoke("kill_terminal", { sessionId: a.sessionId });
    await invoke("kill_terminal", { sessionId: b.sessionId });
    const stopped = await invoke("wait_terminal_exit", {
      sessionId: a.sessionId,
      timeoutMs: 10000,
    });
    check("停止进程", stopped !== null, `exit=${stopped}`);
    const batch = await invoke("run_batch", {
      presetIds: [cmd.id, node.id],
      argsMap: {},
      concurrency: 2,
      continueOnError: false,
    });
    check(
      "批量并发执行",
      batch.sessions.length === 2 && batch.blocked.length === 0,
      JSON.stringify(batch),
    );
    const wf = await invoke("save_workflow", {
      workflow: {
        id: "",
        name: "验收串行工作流",
        description: "",
        runMode: "serial",
        continueOnError: false,
        enabled: true,
        createdAt: 0,
        updatedAt: 0,
        steps: [cmd, node].map((p, i) => ({
          id: "step" + i,
          name: p.name,
          presetId: p.id,
          args: {},
          enabled: true,
          delayMs: 0,
          condition: "always",
        })),
      },
    });
    const wr = await invoke("start_workflow", { id: wf.id });
    let finished;
    for (let i = 0; i < 150; i++) {
      finished = (
        await invoke("list_workflow_runs", { workflowId: wf.id, limit: 5 })
      ).find((r) => r.id === wr.id);
      if (finished.status !== "running") break;
      await new Promise((r) => setTimeout(r, 100));
    }
    check(
      "串行工作流",
      finished.status === "success" && finished.steps.length === 2,
      JSON.stringify(finished),
    );
    check(
      "串行等待上一步结束",
      finished.steps[1].startedAt >= finished.steps[0].finishedAt,
      JSON.stringify(finished.steps),
    );
    const sched = await invoke("save_schedule", {
      schedule: {
        id: "",
        name: "验收定时任务",
        presetId: cmd.id,
        args: {},
        mode: "interval",
        intervalMinutes: 1,
        time: "09:00",
        weekdays: [],
        date: "",
        enabled: true,
        nextRunAt: 0,
        lastRunAt: 0,
        lastStatus: "",
        createdAt: 0,
        updatedAt: 0,
      },
    });
    check(
      "定时任务计算下次时间",
      sched.nextRunAt > Date.now(),
      String(sched.nextRunAt),
    );
    const sid = await invoke("trigger_schedule_now", { id: sched.id });
    const sc = await invoke("wait_terminal_exit", {
      sessionId: sid,
      timeoutMs: 10000,
    });
    check("定时任务立即触发", sc === 0, String(sc));
    const info = await invoke("get_app_info");
    const file = info.dataDir + "\\acceptance-export.json";
    const count = await invoke("export_data", { path: file });
    const imported = await invoke("import_data", {
      path: file,
      mode: "append",
    });
    check(
      "JSON 导出与重复导入",
      count >= 5 && imported.presetsAdded === 0,
      JSON.stringify(imported),
    );
    const audit = await invoke("list_audit_logs");
    const history = await invoke("list_terminal_history", {
      presetId: "",
      limit: 100,
    });
    check(
      "审计与历史落库",
      audit.length >= 8 && history.length >= 7,
      `audit=${audit.length},history=${history.length}`,
    );
    await invoke("delete_preset", { id: denied.id });
    let missing = false;
    try {
      await invoke("get_preset", { id: denied.id });
    } catch (e) {
      missing = true;
    }
    check("删除预设", missing, denied.id);
    await invoke("set_schedule_enabled", { id: sched.id, enabled: false });
    await invoke("save_settings", { settings: original });
    return report;
  });
  return result;
}
