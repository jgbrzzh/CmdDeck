export interface RuntimeBinding {
  kind: "" | "system" | "conda" | "venv" | "python" | "node";
  path: string;
  managerPath: string;
}
export interface EnvironmentInfo extends RuntimeBinding {
  id: string;
  name: string;
  manager: string;
  version: string;
}
export interface ToolInfo {
  name: string;
  path: string;
  version: string;
}
export interface EnvironmentReport {
  tools: ToolInfo[];
  environments: EnvironmentInfo[];
  warnings: string[];
  scannedAt: number;
}
export interface EnvironmentAction {
  tool: string;
  action:
    | "list-packages"
    | "install-package"
    | "create-environment"
    | "install-version";
  package: string;
  name: string;
  version: string;
  projectDir: string;
  runtime: RuntimeBinding;
  confirmed: boolean;
}
