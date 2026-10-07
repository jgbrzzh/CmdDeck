"""更新 Windows 构建依赖清单及原始许可；不把本机缓存路径写入文档。"""
from pathlib import Path
from collections import defaultdict
import hashlib
import json
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]

def main():
    metadata = json.loads(subprocess.check_output([
        "cargo", "metadata", "--locked", "--format-version", "1",
        "--filter-platform", "x86_64-pc-windows-msvc",
        "--manifest-path", str(ROOT / "src-tauri/Cargo.toml"),
    ], cwd=ROOT, encoding="utf-8"))
    # 只列出 Windows 的实际依赖闭包（包含构建工具）。
    used = {node["id"] for node in metadata["resolve"]["nodes"]}
    packages = [("Cargo", p["name"], p["version"], p.get("license") or "未声明", Path(p["manifest_path"]).parent)
                for p in metadata["packages"] if p["id"] in used and p["name"] != "cmddeck"]
    lock = json.loads((ROOT / "package-lock.json").read_text(encoding="utf-8"))
    for relative, entry in lock["packages"].items():
        if not relative:
            continue
        directory = ROOT / relative
        if not directory.is_dir():
            continue
        package_file = directory / "package.json"
        info = json.loads(package_file.read_text(encoding="utf-8")) if package_file.is_file() else {}
        packages.append(("npm", info.get("name", relative.split("node_modules/")[-1]), entry["version"],
                         info.get("license", entry.get("license", "未声明")), directory))
    rows = ["# 锁定依赖清单", "", "由锁文件与 Cargo metadata 生成，包含 Windows 运行及构建依赖和已安装的 npm 依赖。", "",
            "| 来源 | 名称 | 版本 | 许可 |", "| --- | --- | --- | --- |"]
    texts = {}
    labels = defaultdict(list)
    # 保留既有版权原文；旧版本声明留存不会影响当前锁定依赖清单。
    existing = ROOT / "docs/THIRD_PARTY_LICENSES.txt"
    if existing.is_file():
        for header, body in re.findall(r"={70}\n(.*?)\n={70}\n(.*?)(?=\n={70}\n|\Z)", existing.read_text(encoding="utf-8"), re.S):
            if " / " not in header:
                continue
            text = "\n".join(line.rstrip() for line in body.strip().splitlines())
            digest = hashlib.sha256(text.encode()).hexdigest()
            texts[digest] = text
            labels[digest].extend(header.splitlines())
    missing = []
    for source, name, version, license_name, directory in sorted(packages, key=lambda p: (p[0], p[1], p[2])):
        rows.append(f"| {source} | {name} | {version} | {str(license_name).replace('|', '/')} |")
        files = [f for f in directory.iterdir() if f.is_file() and re.match(r"^(licen[sc]e|copying|notice)(?:[._-]|$)", f.name, re.I)]
        if not files:
            missing.append(f"{source} {name} {version} ({license_name})")
        for file in sorted(files):
            text = "\n".join(line.rstrip() for line in file.read_text(encoding="utf-8", errors="replace").splitlines()).strip()
            digest = hashlib.sha256(text.encode()).hexdigest()
            texts[digest] = text
            label = f"{source} {name} {version} / {file.name}"
            if label not in labels[digest]:
                labels[digest].append(label)
    notices = ["第三方原始许可与版权声明。相同许可原文合并，前列适用来源。依赖自身原始文件优先。", ""]
    for digest in labels:
        notices.extend(["=" * 70, *labels[digest], "=" * 70, texts[digest], ""])
    if missing:
        notices.extend(["=" * 70, "以下包未在根目录附带许可原文，许可标识见清单；完整源代码可从对应包仓库获取：", *missing, ""])
    (ROOT / "docs/DEPENDENCIES.md").write_text("\n".join(rows) + "\n", encoding="utf-8")
    (ROOT / "docs/THIRD_PARTY_LICENSES.txt").write_text("\n".join(notices), encoding="utf-8")
    print(f"已更新 {len(packages)} 个依赖、{len(labels)} 份不同的许可文本。")

if __name__ == "__main__":
    main()
