import { invoke } from "@tauri-apps/api/core";
import type {
  EnvironmentReport,
  EnvironmentAction,
  RuntimeBinding,
  TerminalInfo,
} from "@/types";
export const environmentApi = {
  discover: (projectDir = "") =>
    invoke<EnvironmentReport>("discover_environments", { projectDir }),
  openTerminal: (runtime: RuntimeBinding, projectDir: string) =>
    invoke<TerminalInfo>("open_environment_terminal", { runtime, projectDir }),
  runAction: (request: EnvironmentAction) =>
    invoke<TerminalInfo>("run_environment_action", { request }),
};
