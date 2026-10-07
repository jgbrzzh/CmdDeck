// 通过 Playwright CLI 连接真实 Tauri 开发窗口；只允许隔离测试目录。
// Node 是必需项；Conda / 项目 .venv 不存在时明确记为未测试。
async (page) => {
  return await page.evaluate(async () => {
    const invoke = async (cmd, args) => {
      try {
        return await window.__TAURI_INTERNALS__.invoke(cmd, args);
      } catch (e) {
        throw new Error(cmd + ": " + JSON.stringify(e));
      }
    };
    const info = await invoke("get_app_info");
    if (!/[\\/]test-results[\\/]/.test(info.dataDir))
      throw new Error("请使用 test-results 下的独立 CMDDECK_DATA_DIR");
    const projectDir = info.dataDir + "\\env-fixture";
    const report = await invoke("discover_environments", { projectDir });
    // 开发服务器热更新会给模块附加版本参数，必须读取应用实际使用的单例。
    const appSource = await (await fetch("/src/App.vue")).text();
    const storeUrl = appSource.match(
      /from "([^"]*\/stores\/terminals\.ts[^\"]*)"/,
    )[1];
    const { useTerminalStore } = await import(storeUrl);
    const results = [],
      skipped = [];
    const check = (name, ok, detail) => {
      results.push({ name, ok, detail });
      if (!ok) throw new Error(name + ": " + JSON.stringify(detail));
    };
    const binding = (e) => ({
      kind: e.kind,
      path: e.path,
      managerPath: e.managerPath,
    });
    const template = (await invoke("list_presets"))[0];
    const create = (name, kind, args, runtime) =>
      invoke("save_preset", {
        preset: {
          ...template,
          id: "",
          name: "环境验收 " + name,
          kind,
          program: "",
          args,
          runtime,
          workingDir: "",
          env: [],
          useShell: false,
          confirm: false,
          elevated: false,
          favorite: false,
        },
      });
    const run = async (preset, expected) => {
      const session = await invoke("run_preset", {
        presetId: preset.id,
        args: {},
        confirmed: false,
      });
      const code = await invoke("wait_terminal_exit", {
        sessionId: session.sessionId,
        timeoutMs: 45000,
      });
      const output = await invoke("get_terminal_snapshot", {
        sessionId: session.sessionId,
      });
      check(preset.name, code === 0 && output.includes(expected), { code });
      return session;
    };
    const node = report.environments.find((e) => e.kind === "node");
    check("检测 Node", !!node, "需要在 PATH 中安装 Node");
    const marker = "TAIL_AFTER_BUFFER_LIMIT";
    const code = String.raw`process.stdout.write(('x'.repeat(60)+'\n').repeat(40000)); setTimeout(()=>console.log('TAIL_AFTER_BUFFER_LIMIT'),300);`;
    let resizeCalls = 0,
      dataEvents = 0;
    const base = window.__TAURI_INTERNALS__.invoke.bind(
      window.__TAURI_INTERNALS__,
    );
    const { onPtyData } = await import("/src/api/events.ts");
    const unlisten = await onPtyData(() => dataEvents++);
    window.__TAURI_INTERNALS__.invoke = (cmd, args, opts) => {
      if (cmd === "resize_terminal") resizeCalls++;
      return base(cmd, args, opts);
    };
    try {
      const p = await create("输出上限", "node", ["-e", code], binding(node));
      const session = await run(p, marker);
      await new Promise((r) => setTimeout(r, 500));
      const tab = useTerminalStore().byId(session.sessionId);
      check(
        "缓存淘汰后继续显示输出",
        tab.outputOffset > 0 &&
          tab.outputBuffer.length <= 1048576 &&
          tab.outputBuffer.includes(marker),
        { buffer: tab.outputBuffer.length },
      );
      check(
        "真实 xterm 尾部标记",
        [...document.querySelectorAll(".xterm-rows")].some((el) =>
          el.textContent.includes(marker),
        ),
        "持续显示最新输出",
      );
      check("输出不触发逐块重排", resizeCalls < 10 && dataEvents > 10, {
        resizeCalls,
        dataEvents,
      });
    } finally {
      unlisten();
      window.__TAURI_INTERNALS__.invoke = base;
    }
    const venv = report.environments.find((e) => e.kind === "venv");
    if (venv) {
      const p = await create(
        "虚拟环境",
        "python",
        [
          "-c",
          "import os,sys; print('VENV_OK'); print(os.environ.get('VIRTUAL_ENV'))",
        ],
        binding(venv),
      );
      await run(p, "VENV_OK");
      const saved = await invoke("get_preset", { id: p.id });
      check(
        "环境绑定保存",
        saved.runtime.path === venv.path,
        "SQLite roundtrip",
      );
    } else skipped.push("项目 .venv 未安装");
    const python = report.tools.find((t) => t.name === "python");
    const conda = report.environments.find(
      (e) =>
        e.kind === "conda" &&
        python?.path.toLowerCase().startsWith(e.path.toLowerCase()),
    );
    if (conda) {
      const p = await create(
        "Conda",
        "python",
        ["-c", "print('CONDA_OK')"],
        binding(conda),
      );
      await run(p, "CONDA_OK");
    } else skipped.push("未检测到 PATH Python 所在的 Conda 环境");
    return { results, skipped, toolCount: report.tools.length };
  });
}
