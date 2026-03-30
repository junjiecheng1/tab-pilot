"""
Aily 模块深度内省脚本
在 Aily 沙箱（Linux + Python 3.10）中运行：
  python3 /home/workspace/inspect_aily_modules.py

输出到: /home/workspace/artifacts/module_inspect/
"""
import importlib, inspect, os, json, sys, traceback

MODULES = [
    # (包名, 子模块列表)
    ("aily_base", ["main", "utils", "parser",
                   "commands.create", "commands.delete", "commands.export",
                   "commands.file_download", "commands.file_upload",
                   "commands.info", "commands.sync"]),
    ("aily_calendar", ["main"]),
    ("aily_chart", ["main", "utils",
                    "commands.area", "commands.bar", "commands.box",
                    "commands.funnel", "commands.heatmap", "commands.line",
                    "commands.pie", "commands.radar", "commands.scatter",
                    "commands.treemap"]),
    ("aily_diagram", ["main", "utils",
                      "commands.classdiagram", "commands.er", "commands.flowchart",
                      "commands.gantt", "commands.mindmap", "commands.sequence",
                      "commands.state"]),
    ("aily_doc", ["main", "utils",
                  "commands.comments", "commands.info", "commands.list",
                  "commands.search"]),
    ("aily_im", ["main", "utils",
                 "commands._format", "commands.chats", "commands.messages"]),
    ("aily_mcp", ["main", "commands.mcp"]),
    ("aily_pdf", ["main", "output",
                  "cmd_convert", "cmd_extract", "cmd_form",
                  "cmd_html", "cmd_latex", "cmd_meta", "cmd_pages"]),
    ("aily_user", ["main"]),
]

OUT_DIR = "/home/workspace/artifacts/module_inspect"


def inspect_callable(obj, name, indent=""):
    """提取函数/方法的完整信息"""
    result = {"name": name, "type": type(obj).__name__}

    # 签名
    try:
        sig = inspect.signature(obj)
        result["signature"] = str(sig)
        result["params"] = {}
        for pname, param in sig.parameters.items():
            p = {"kind": str(param.kind.name)}
            if param.default is not inspect.Parameter.empty:
                try:
                    p["default"] = repr(param.default)
                except:
                    p["default"] = str(param.default)
            if param.annotation is not inspect.Parameter.empty:
                try:
                    p["annotation"] = str(param.annotation)
                except:
                    pass
            result["params"][pname] = p
    except (ValueError, TypeError):
        result["signature"] = "(无法获取)"

    # Docstring
    doc = inspect.getdoc(obj)
    if doc:
        result["docstring"] = doc

    # 源码（大概率失败，但试一下）
    try:
        src = inspect.getsource(obj)
        result["source"] = src
    except (OSError, TypeError):
        pass

    return result


def inspect_class(cls, name):
    """提取类的完整信息"""
    result = {
        "name": name,
        "type": "class",
        "bases": [b.__name__ for b in cls.__bases__],
        "mro": [c.__name__ for c in cls.__mro__],
        "methods": {},
        "attributes": {},
    }

    doc = inspect.getdoc(cls)
    if doc:
        result["docstring"] = doc

    for attr_name in sorted(dir(cls)):
        if attr_name.startswith("__") and attr_name.endswith("__"):
            continue
        try:
            attr = getattr(cls, attr_name)
        except:
            continue

        if callable(attr):
            result["methods"][attr_name] = inspect_callable(attr, attr_name)
        else:
            try:
                result["attributes"][attr_name] = repr(attr)
            except:
                result["attributes"][attr_name] = str(type(attr))

    return result


def inspect_click_command(cmd, prefix=""):
    """递归提取 Click 命令树"""
    result = {
        "name": cmd.name,
        "full_command": f"{prefix} {cmd.name}".strip(),
    }

    if hasattr(cmd, "help") and cmd.help:
        result["help"] = cmd.help

    # 参数
    if hasattr(cmd, "params"):
        result["params"] = []
        for param in cmd.params:
            p = {
                "name": param.name,
                "type": str(param.type),
                "required": getattr(param, "required", False),
            }
            if hasattr(param, "help") and param.help:
                p["help"] = param.help
            if hasattr(param, "default") and param.default is not None:
                try:
                    p["default"] = repr(param.default)
                except:
                    pass
            if hasattr(param, "opts"):
                p["opts"] = list(param.opts)
            result["params"].append(p)

    # 子命令
    if hasattr(cmd, "commands"):
        result["subcommands"] = {}
        for name, subcmd in cmd.commands.items():
            result["subcommands"][name] = inspect_click_command(
                subcmd, f"{prefix} {cmd.name}".strip()
            )

    return result


def inspect_module(pkg_name, sub_name):
    """内省一个模块"""
    full_name = f"{pkg_name}.{sub_name}"
    result = {
        "module": full_name,
        "functions": {},
        "classes": {},
        "constants": {},
        "click_commands": {},
    }

    try:
        mod = importlib.import_module(full_name)
    except Exception as e:
        result["error"] = f"导入失败: {e}"
        return result

    # 模块级 docstring
    if mod.__doc__:
        result["module_doc"] = mod.__doc__

    for name in sorted(dir(mod)):
        if name.startswith("_"):
            continue

        try:
            obj = getattr(mod, name)
        except:
            continue

        # Click 命令
        if hasattr(obj, "commands") or (
            callable(obj) and hasattr(obj, "params") and hasattr(obj, "name")
        ):
            try:
                result["click_commands"][name] = inspect_click_command(obj)
            except:
                pass

        # 类
        elif inspect.isclass(obj):
            result["classes"][name] = inspect_class(obj, name)

        # 函数
        elif callable(obj):
            result["functions"][name] = inspect_callable(obj, name)

        # 常量
        elif isinstance(obj, (str, int, float, bool, list, dict, tuple)):
            try:
                result["constants"][name] = repr(obj)[:500]
            except:
                pass

    return result


def result_to_python(result, pkg, sub):
    """将提取结果转为可读的 Python 伪代码"""
    lines = []
    full_name = f"{pkg}.{sub}"
    lines.append(f'"""')
    lines.append(f"模块: {full_name}")
    lines.append(f"提取方式: Python inspect（沙箱内 import）")
    if result.get("module_doc"):
        lines.append(f"")
        lines.append(result["module_doc"])
    lines.append(f'"""')
    lines.append("")

    # Imports（从类的 bases 推断）
    imports = set()
    for cls_data in result.get("classes", {}).values():
        for base in cls_data.get("bases", []):
            if base not in ("object",):
                imports.add(base)
    if imports:
        lines.append("# ===== 推断的依赖 =====")
        for imp in sorted(imports):
            lines.append(f"# from ... import {imp}")
        lines.append("")

    # 常量
    if result.get("constants"):
        lines.append("# ===== 常量 =====")
        for name, val in result["constants"].items():
            lines.append(f"{name} = {val}")
        lines.append("")

    # 类
    for cls_name, cls_data in result.get("classes", {}).items():
        bases = ", ".join(cls_data.get("bases", ["object"]))
        lines.append(f"class {cls_name}({bases}):")
        if cls_data.get("docstring"):
            lines.append(f'    """{cls_data["docstring"]}"""')
        lines.append("")

        for attr_name, attr_val in cls_data.get("attributes", {}).items():
            lines.append(f"    {attr_name} = {attr_val}")
        if cls_data.get("attributes"):
            lines.append("")

        for meth_name, meth_data in cls_data.get("methods", {}).items():
            sig = meth_data.get("signature", "()")
            lines.append(f"    def {meth_name}{sig}:")
            if meth_data.get("docstring"):
                lines.append(f'        """{meth_data["docstring"]}"""')
            if meth_data.get("source"):
                for src_line in meth_data["source"].split("\n"):
                    lines.append(f"        {src_line}")
            else:
                lines.append(f"        ...  # Cython 编译，函数体不可用")
            lines.append("")
        lines.append("")

    # 函数
    for func_name, func_data in result.get("functions", {}).items():
        sig = func_data.get("signature", "()")
        lines.append(f"def {func_name}{sig}:")
        if func_data.get("docstring"):
            lines.append(f'    """{func_data["docstring"]}"""')
        if func_data.get("source"):
            for src_line in func_data["source"].split("\n"):
                lines.append(f"    {src_line}")
        else:
            lines.append(f"    ...  # Cython 编译，函数体不可用")
        lines.append("")

    # Click 命令
    for cmd_name, cmd_data in result.get("click_commands", {}).items():
        lines.append(f"# ===== Click 命令: {cmd_name} =====")
        lines.append(f"# 命令: {cmd_data.get('full_command', cmd_name)}")
        if cmd_data.get("help"):
            lines.append(f"# 帮助: {cmd_data['help']}")

        for param in cmd_data.get("params", []):
            opts = " / ".join(param.get("opts", [param["name"]]))
            req = "必填" if param.get("required") else "可选"
            help_text = param.get("help", "")
            default = param.get("default", "")
            lines.append(
                f"#   {opts}: type={param['type']}, {req}"
                + (f", default={default}" if default else "")
                + (f" — {help_text}" if help_text else "")
            )

        # 递归子命令
        for sub_name, sub_data in cmd_data.get("subcommands", {}).items():
            lines.append(f"#   子命令: {sub_data.get('full_command', sub_name)}")
            if sub_data.get("help"):
                lines.append(f"#     帮助: {sub_data['help']}")
            for param in sub_data.get("params", []):
                opts = " / ".join(param.get("opts", [param["name"]]))
                req = "必填" if param.get("required") else "可选"
                help_text = param.get("help", "")
                lines.append(
                    f"#     {opts}: type={param['type']}, {req}"
                    + (f" — {help_text}" if help_text else "")
                )
        lines.append("")

    return "\n".join(lines)


def main():
    os.makedirs(OUT_DIR, exist_ok=True)

    for pkg_name, sub_modules in MODULES:
        print(f"\n{'='*60}")
        print(f"📦 处理: {pkg_name}")
        print(f"{'='*60}")

        pkg_dir = os.path.join(OUT_DIR, pkg_name)
        os.makedirs(pkg_dir, exist_ok=True)

        for sub in sub_modules:
            full_name = f"{pkg_name}.{sub}"
            print(f"  🔍 {full_name}...", end=" ")

            try:
                result = inspect_module(pkg_name, sub)

                if result.get("error"):
                    print(f"❌ {result['error']}")
                    continue

                # 保存 JSON（完整数据）
                json_path = os.path.join(pkg_dir, f"{sub.replace('.', '_')}.json")
                with open(json_path, "w", encoding="utf-8") as f:
                    json.dump(result, f, ensure_ascii=False, indent=2)

                # 保存 Python 伪代码（可读版本）
                py_path = os.path.join(pkg_dir, f"{sub.replace('.', '_')}.py")
                py_content = result_to_python(result, pkg_name, sub)
                with open(py_path, "w", encoding="utf-8") as f:
                    f.write(py_content)

                n_funcs = len(result.get("functions", {}))
                n_classes = len(result.get("classes", {}))
                n_clicks = len(result.get("click_commands", {}))
                print(f"✅ {n_funcs}函数 {n_classes}类 {n_clicks}CLI命令")

            except Exception as e:
                print(f"❌ {e}")
                traceback.print_exc()

    print(f"\n✅ 全部完成! 输出目录: {OUT_DIR}")
    print(f"   JSON: 完整数据")
    print(f"   .py:  可读伪代码")


if __name__ == "__main__":
    main()
