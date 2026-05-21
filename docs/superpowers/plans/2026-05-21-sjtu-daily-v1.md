# sjtu-daily v1 实施 Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Windows 本地构建一个每日待办 dashboard —— Task Scheduler 每天 7:00 跑一次（开机补跑），调用既有 sjtu-cli 拉 5 个子系统数据，diff 出新增项后生成本地 HTML + Windows Toast。

**Architecture:** 纯本地 Python 单进程。**SJTU-CLI 源码 0 行修改**，新建独立项目 `~/sjtu-daily/`。subprocess 调 sjtu.exe 受白名单 `safety.ALLOWED` 死锁；SQLite 只存 `(category, item_id, notified_at)` 三元组 metadata，邮件正文 / 帖子原文 / 发件人 **绝不持久化**。HTML 模板只展示标题 + ID + 时间。失败走 Toast 提示 "请运行 sjtu login"，不静默吞。

**Tech Stack:** Python 3.11+（std `tomllib`）/ PyYAML / Jinja2 / windows-toasts / sqlite3（std）/ pytest + pytest-mock / PowerShell（Task Scheduler 安装）

---

## 红线契约（implementer 必读，违者 NACK）

1. **零侵入**：本 plan **不修改** `E:\claude_ask\sjtu_CLI\sjtu-cli\` 下任何文件。所有产出在 `~/sjtu-daily/`（即 `C:\Users\<your-username>\sjtu-daily\`）。
2. **命令白名单**：`safety.ALLOWED` 是 `frozenset[tuple[str, ...]]`，元组是 argv 精确字面值。任何 argv 不在白名单 → `SafetyViolation` 异常。
3. **永不调 `sjtu messages show`**：会触发服务端**隐式标已读**副作用（见 `src/commands/jwbmessage/data.rs:30` `side_effect_marked_read`）。v1 只用 `messages list --unread-only` 拿 group 元数据。
4. **永不调任何写命令**：白名单只含 5 个读命令；编译期 grep 白名单不能出现 `auth` / `reply` / `like` / `read-all` / `delete-*` / `pm-send` / `archive-pm` / `setup` / `download` 等动词。
5. **SQLite schema 强制最小**：`seen(category TEXT, item_id TEXT, first_seen_at TEXT, notified_at TEXT)`。**禁列**：`subject` / `body` / `from` / `body_plain` / `fragment` / `excerpt` / `title`。违反由 `state.py` 单测守门。
6. **HTML 不出 PII**：`dashboard.html` 渲染时只允许 `(item_id, title_or_subject, date_local, is_new)` 字段，**不出**：`from_address` / `from_display` / `body_plain` / `fragment` / `excerpt`（即便 sjtu 返了也要在 `runner.py` drop）。
7. **subprocess 超时**：每个 sjtu CLI 调用 30s 硬上限，超时算失败，不重试。
8. **session 过期不静默**：subprocess returncode != 0 且 stderr 含 `SessionExpired` 字串 → Toast "请运行 sjtu login"，dashboard 显示该 category 状态 = `auth_required`，**不写入 seen 表**（否则下次 login 后会漏推上次过期期间的新增）。
9. **金额永不进 float**：card balance 的 `balance` / `trans_balance` 字段在 sjtu envelope 里是 Decimal 序列化成 `"123.45"` 字符串。Python 端用 `decimal.Decimal(s)` 接，**不能** `float(s)`。dashboard 用 `f"{d:.2f}"` 渲染。
10. **不调 LLM**：v1 不集成 Ollama / Gemini / 任何外部 API。LLM 是 v2 范围。
11. **不写 cookie / token 到 dashboard / SQLite / 日志**：sjtu envelope 本身不含原始 token，但 `meta.via` 之类字段允许出。

---

## 文件结构（v1）

```
C:\Users\<your-username>\sjtu-daily\                  # 项目根（新 git 仓 ~/sjtu-daily/）
├── .gitignore                              # data/ / *.db / dashboard.html / dist/
├── README.md                               # 用户使用文档
├── LICENSE                                 # MIT
├── pyproject.toml                          # Python 项目配置 + 依赖
├── config.example.toml                     # 配置示例
├── src/
│   └── sjtu_daily/
│       ├── __init__.py                     # __version__ = "0.1.0"
│       ├── __main__.py                     # python -m sjtu_daily
│       ├── cli.py                          # argparse: run / dry-run / version
│       ├── safety.py                       # ALLOWED 白名单 + validate_argv
│       ├── runner.py                       # subprocess 5 调 + YAML 解析 + Snapshot dataclass
│       ├── state.py                        # SQLite schema + diff + last_run_at
│       ├── render.py                       # Jinja2 → dashboard.html
│       ├── notify.py                       # Windows Toast 包装
│       ├── config.py                       # tomllib 读 config.toml
│       ├── paths.py                        # 跨平台 data_dir / config_path / dashboard_path
│       └── templates/
│           └── dashboard.html.j2           # Jinja2 模板
├── tests/
│   ├── __init__.py
│   ├── conftest.py                         # pytest fixtures（临时 data_dir）
│   ├── test_safety.py                      # 白名单守卫单测
│   ├── test_runner.py                      # subprocess mock + 解析
│   ├── test_state.py                       # SQLite schema + diff
│   ├── test_render.py                      # HTML 渲染断言
│   ├── test_config.py                      # toml 读
│   ├── test_paths.py                       # 跨平台路径
│   ├── test_cli.py                         # CLI 入口 dry-run
│   └── fixtures/
│       ├── envelope_mail_list.yaml         # 真机 dry-run 脱敏拷贝
│       ├── envelope_messages_list.yaml
│       ├── envelope_services_pending.yaml
│       ├── envelope_shuiyuan_latest.yaml
│       ├── envelope_card_balance.yaml
│       └── envelope_error_session_expired.yaml
└── scripts/
    ├── install-task.ps1                    # 创建 Windows Task Scheduler 任务
    └── uninstall-task.ps1                  # 卸载
```

---

## SJTU-CLI Envelope Schema 速查（已对齐源码 2026-05-21）

implementer 解析时直接 `yaml.safe_load(stdout)` 取 dict，**不强求 dataclass 严格映射**，只挑下列字段。其他字段 drop（红线 6）。

### mail list

```yaml
ok: true
schema_version: "1"
data:
  query: "in:inbox"
  count: 5
  offset: 0
  has_more: false
  items:
    - id: "12345"
      subject: "通知：..."
      date_ms: 1716268800000
      unread: true
      # ⚠️ 以下字段虽然 envelope 会出，runner.py 必须 drop：
      # from_display, from_address, fragment, size_bytes
```

→ Python 取：`id`, `subject`, `date_ms`, `unread`。

### messages list（jwbmessage，snake_case）

```yaml
ok: true
data:
  page: 1
  unread_only: true
  returned: 3
  total: 3
  groups:
    - group_id: "ABC123"
      group_name: "教学秘书通知"
      unread_num: 2
      group_description: "..."
      is_group: true
      is_read: false
      create_time: "2026-05-21 08:00:00"
```

→ Python 取：`group_id`, `group_name`, `unread_num`, `create_time`。**不调 `messages show`**（副作用红线 3）。

### services pending（camelCase）

```yaml
ok: true
data:
  total: 2
  returned: 2
  with_identity: false
  my_applications:
    - id: "step-uuid-1"
      name: "填写申请"
      code: "ADD"
      assignTime: 1716268800
      process:
        id: "proc-uuid-1"
        name: "学位申请"
        entry: "20054472"
        update: 1716268900
        status: "doing"
        app:
          code: "HXBDSQ"
          name: "学位评定"
  awaiting_my_action:
    - id: "step-uuid-2"
      name: "审核"
      code: "REVIEW"
      assignTime: 1716268700
      process:
        id: "proc-uuid-2"
        name: "请假申请审核"
```

→ Python 取：`id`, `process.name` (作为标题), `process.app.name`, `assignTime`。两个列表分别渲染。**绝不**取 `process.owner.name` / `process.owner.id`（PII）。

### shuiyuan latest

```yaml
ok: true
data:
  page: 0
  returned: 30
  per_page: 30
  more_topics_url: "/latest.json?page=1"
  topics:
    - id: 123456
      title: "..."
      fancy_title: "..."
      posts_count: 5
      reply_count: 4
      views: 100
      like_count: 8
      last_posted_at: "2026-05-21T08:00:00.000Z"
      excerpt: "..."
      tags: []
```

→ Python 取：`id`, `title`, `last_posted_at`, `reply_count`。**drop**: `excerpt`（红线 6）。

### card balance

```yaml
ok: true
schema_version: "1"
data:
  card_no_redacted: "0012***"
  balance: "123.45"        # 字符串！必须 Decimal()
  trans_balance: "0.00"
  expire_date: "2027-09-01"
  lost: false
  frozen: false
  face_type: "..."
  from_cache: false
  elapsed_ms: 1234
meta:
  via: "weixin"            # 可能没有这个键（auto path 走 oauth2 时也可能有）
  source_hint: "weixin.sjtu.edu.cn"
```

→ Python 取：`card_no_redacted`, `balance` (Decimal), `lost`, `frozen`。

### 错误信封（session 过期）

```yaml
ok: false
schema_version: "1"
error:
  code: "session-expired"
  message: "..."
```

stderr 也常含 `SessionExpired` 关键字。runner 双重检查（envelope `ok=false` + stderr 关键字）。

---

## Task 0: 项目骨架

**Files:**
- Create: `C:\Users\<your-username>\sjtu-daily\.gitignore`
- Create: `C:\Users\<your-username>\sjtu-daily\pyproject.toml`
- Create: `C:\Users\<your-username>\sjtu-daily\config.example.toml`
- Create: `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\__init__.py`
- Create: `C:\Users\<your-username>\sjtu-daily\LICENSE`

- [ ] **Step 1: 建目录 + git init**

```powershell
New-Item -ItemType Directory -Force C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\templates
New-Item -ItemType Directory -Force C:\Users\<your-username>\sjtu-daily\tests\fixtures
New-Item -ItemType Directory -Force C:\Users\<your-username>\sjtu-daily\scripts
git -C C:\Users\<your-username>\sjtu-daily init
```

- [ ] **Step 2: 写 `.gitignore`**

文件 `C:\Users\<your-username>\sjtu-daily\.gitignore`：

```
# Python
__pycache__/
*.py[cod]
*.egg-info/
.pytest_cache/
.mypy_cache/
.venv/
dist/
build/

# sjtu-daily 数据（红线：含状态 DB 和渲染产物，绝不入 git）
data/
*.db
*.db-journal
dashboard.html

# OS
.DS_Store
Thumbs.db

# IDE
.vscode/
.idea/
*.swp
```

- [ ] **Step 3: 写 `pyproject.toml`**

文件 `C:\Users\<your-username>\sjtu-daily\pyproject.toml`：

```toml
[project]
name = "sjtu-daily"
version = "0.1.0"
description = "本地每日待办 dashboard，调用 SJTU-CLI 5 个子系统聚合"
requires-python = ">=3.11"
license = { text = "MIT" }
authors = [{ name = "wuyutanhongyuxin", email = "wuyutanhongyuxin@gmail.com" }]
dependencies = [
    "pyyaml>=6.0",
    "jinja2>=3.1",
    "windows-toasts>=1.1.0; sys_platform == 'win32'",
]

[project.optional-dependencies]
dev = [
    "pytest>=8.0",
    "pytest-mock>=3.12",
]

[project.scripts]
sjtu-daily = "sjtu_daily.cli:main"

[build-system]
requires = ["setuptools>=68.0"]
build-backend = "setuptools.build_meta"

[tool.setuptools.packages.find]
where = ["src"]

[tool.setuptools.package-data]
sjtu_daily = ["templates/*.j2"]

[tool.pytest.ini_options]
testpaths = ["tests"]
pythonpath = ["src"]
```

- [ ] **Step 4: 写 `config.example.toml`**

文件 `C:\Users\<your-username>\sjtu-daily\config.example.toml`：

```toml
# sjtu-daily 配置示例。
# 真实使用时复制到 ~/sjtu-daily/config.toml 并按需修改。

[sjtu_cli]
# sjtu.exe 绝对路径。空字符串则从 PATH 找。
binary = "E:/claude_ask/sjtu_CLI/sjtu-cli/target/release/sjtu.exe"
# 每个 sjtu 子命令超时秒数（runner 红线 7）。
timeout_seconds = 30

[mail]
# unread 拉取上限
limit = 50

[shuiyuan]
# 最新帖拉取上限
limit = 30

[notify]
# Windows Toast 应用名（显示在系统通知中心）
app_name = "SJTU Daily"

[scheduler]
# wrapper 内部最小间隔：距上次成功 run < min_interval_hours 直接 silent exit
# （配合 Task Scheduler 双 trigger 7:00 + AtLogon 防重复跑）
min_interval_hours = 6
```

- [ ] **Step 5: 写 `src/sjtu_daily/__init__.py`**

文件 `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\__init__.py`：

```python
"""sjtu-daily — Windows 本地每日待办 dashboard。

调用既有 SJTU-CLI（Rust 二进制）拉 5 个子系统数据，diff 新增项后生成
本地 HTML + Windows Toast 通知。绝不修改 sjtu-cli 源码。
"""

__version__ = "0.1.0"
```

- [ ] **Step 6: 写 `LICENSE`**

文件 `C:\Users\<your-username>\sjtu-daily\LICENSE`（标准 MIT 文本，版权人写 `wuyutanhongyuxin`）。

- [ ] **Step 7: 装 dev 依赖 + 冒烟**

```powershell
cd C:\Users\<your-username>\sjtu-daily
python -m venv .venv
.\.venv\Scripts\Activate.ps1
pip install -e .[dev]
python -c "import sjtu_daily; print(sjtu_daily.__version__)"
```

Expected: `0.1.0`

- [ ] **Step 8: Commit**

```powershell
cd C:\Users\<your-username>\sjtu-daily
git add .gitignore pyproject.toml config.example.toml LICENSE src/sjtu_daily/__init__.py
git commit -m "chore: 项目骨架 + Python 3.11+ + setuptools"
```

---

## Task 1: paths.py（跨平台路径）

**Files:**
- Create: `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\paths.py`
- Create: `C:\Users\<your-username>\sjtu-daily\tests\__init__.py`
- Create: `C:\Users\<your-username>\sjtu-daily\tests\conftest.py`
- Create: `C:\Users\<your-username>\sjtu-daily\tests\test_paths.py`

- [ ] **Step 1: 写失败测试 `tests/test_paths.py`**

```python
"""paths 模块测试 —— 跨平台路径分辨。"""
from pathlib import Path

from sjtu_daily import paths


def test_project_root_uses_userprofile_env(monkeypatch, tmp_path):
    """project_root() 应优先用 SJTU_DAILY_HOME；否则 ~/sjtu-daily/。"""
    monkeypatch.setenv("SJTU_DAILY_HOME", str(tmp_path))
    assert paths.project_root() == tmp_path


def test_project_root_defaults_to_home(monkeypatch, tmp_path):
    """没有 SJTU_DAILY_HOME 时落 ~/sjtu-daily/。"""
    monkeypatch.delenv("SJTU_DAILY_HOME", raising=False)
    monkeypatch.setenv("USERPROFILE", str(tmp_path))
    monkeypatch.setenv("HOME", str(tmp_path))
    assert paths.project_root() == tmp_path / "sjtu-daily"


def test_data_dir_under_root(monkeypatch, tmp_path):
    monkeypatch.setenv("SJTU_DAILY_HOME", str(tmp_path))
    assert paths.data_dir() == tmp_path / "data"


def test_db_path(monkeypatch, tmp_path):
    monkeypatch.setenv("SJTU_DAILY_HOME", str(tmp_path))
    assert paths.db_path() == tmp_path / "data" / "state.db"


def test_dashboard_path(monkeypatch, tmp_path):
    monkeypatch.setenv("SJTU_DAILY_HOME", str(tmp_path))
    assert paths.dashboard_path() == tmp_path / "dashboard.html"


def test_config_path(monkeypatch, tmp_path):
    monkeypatch.setenv("SJTU_DAILY_HOME", str(tmp_path))
    assert paths.config_path() == tmp_path / "config.toml"


def test_ensure_data_dir_creates(monkeypatch, tmp_path):
    monkeypatch.setenv("SJTU_DAILY_HOME", str(tmp_path))
    assert not (tmp_path / "data").exists()
    paths.ensure_data_dir()
    assert (tmp_path / "data").is_dir()
```

- [ ] **Step 2: 跑测试，确认全部 FAIL**

```powershell
cd C:\Users\<your-username>\sjtu-daily
.\.venv\Scripts\Activate.ps1
pytest tests/test_paths.py -v
```

Expected: 7 个测试全部 FAIL，原因 `ModuleNotFoundError` 或 `AttributeError`。

- [ ] **Step 3: 写 conftest.py 跨测试通用 fixture**

文件 `C:\Users\<your-username>\sjtu-daily\tests\__init__.py`：空文件。

文件 `C:\Users\<your-username>\sjtu-daily\tests\conftest.py`：

```python
"""pytest 通用 fixtures。"""
import pytest


@pytest.fixture
def sjtu_daily_home(monkeypatch, tmp_path):
    """临时 SJTU_DAILY_HOME，自动隔离各测试。"""
    monkeypatch.setenv("SJTU_DAILY_HOME", str(tmp_path))
    return tmp_path
```

- [ ] **Step 4: 实现 `paths.py`**

文件 `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\paths.py`：

```python
"""跨平台路径分辨。

项目根（project_root）优先级：
1. 环境变量 SJTU_DAILY_HOME（测试用 / 用户自定义）
2. ~/sjtu-daily/（默认）

所有派生路径都在 project_root 下，便于整体迁移 / 备份。
"""
import os
from pathlib import Path


def project_root() -> Path:
    """项目根目录。SJTU_DAILY_HOME 优先；否则 ~/sjtu-daily/。"""
    override = os.environ.get("SJTU_DAILY_HOME")
    if override:
        return Path(override)
    return Path.home() / "sjtu-daily"


def data_dir() -> Path:
    """状态数据目录（SQLite / 临时文件）。"""
    return project_root() / "data"


def db_path() -> Path:
    """SQLite 状态文件路径。"""
    return data_dir() / "state.db"


def dashboard_path() -> Path:
    """渲染好的 dashboard.html 路径。"""
    return project_root() / "dashboard.html"


def config_path() -> Path:
    """config.toml 路径。"""
    return project_root() / "config.toml"


def ensure_data_dir() -> None:
    """确保 data_dir 存在（mkdir -p 等价）。"""
    data_dir().mkdir(parents=True, exist_ok=True)
```

- [ ] **Step 5: 跑测试，确认全部 PASS**

```powershell
pytest tests/test_paths.py -v
```

Expected: 7 passed。

- [ ] **Step 6: Commit**

```powershell
cd C:\Users\<your-username>\sjtu-daily
git add src/sjtu_daily/paths.py tests/__init__.py tests/conftest.py tests/test_paths.py
git commit -m "feat: paths.py 跨平台路径分辨 + SJTU_DAILY_HOME override"
```

---

## Task 2: safety.py（命令白名单 —— 核心红线）

**Files:**
- Create: `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\safety.py`
- Create: `C:\Users\<your-username>\sjtu-daily\tests\test_safety.py`

- [ ] **Step 1: 写失败测试 `tests/test_safety.py`**

```python
"""safety 模块测试 —— 白名单是核心红线，必须严格守门。"""
import pytest

from sjtu_daily.safety import (
    ALLOWED,
    SafetyViolation,
    build_safe_argv,
    validate_argv,
)


# ============== ALLOWED 内容守恒 ==============

def test_allowed_is_exactly_five_commands():
    """白名单恰好 5 条（红线 4：v1 锁死，加白单要改测试）。"""
    assert len(ALLOWED) == 5


def test_allowed_no_write_verbs():
    """白名单不能含任何写动词。每加白单都要过这个测试。"""
    forbidden = {
        "auth", "setup", "reply", "like", "read-all",
        "delete-topic", "delete-post", "pm-send", "archive-pm",
        "new-topic", "download", "logout", "show",  # show 含已读副作用
    }
    for argv in ALLOWED:
        assert not (forbidden & set(argv)), f"白名单 {argv} 含禁词"


# ============== validate_argv ==============

def test_validate_accepts_whitelisted_services():
    assert validate_argv(["services", "pending", "--yaml"]) is True


def test_validate_accepts_whitelisted_messages():
    assert validate_argv(["messages", "list", "--unread-only", "--yaml"]) is True


def test_validate_accepts_whitelisted_mail():
    assert validate_argv(["mail", "list", "--unread", "--limit", "50", "--yaml"]) is True


def test_validate_accepts_whitelisted_shuiyuan():
    assert validate_argv(["shuiyuan", "latest", "--limit", "30", "--yaml"]) is True


def test_validate_accepts_whitelisted_card():
    assert validate_argv(["card", "balance", "--yaml"]) is True


def test_validate_rejects_write_command_card_auth():
    """red line: card auth 是 OAuth2 写流程，必须拒。"""
    assert validate_argv(["card", "auth", "--client-id", "x"]) is False


def test_validate_rejects_messages_show():
    """red line: messages show 触发服务端标已读。"""
    assert validate_argv(["messages", "show", "ABC123"]) is False


def test_validate_rejects_messages_read_all():
    assert validate_argv(["messages", "read-all"]) is False


def test_validate_rejects_extra_args():
    """精确匹配：多一个参数都拒。"""
    assert validate_argv(["mail", "list", "--unread", "--limit", "50", "--yaml", "extra"]) is False


def test_validate_rejects_shell_injection():
    """shell 元字符不应该到这层（subprocess 不走 shell），但守门防策划失误。"""
    assert validate_argv(["mail", "list", ";", "rm", "-rf", "/"]) is False
    assert validate_argv(["mail", "list", "&&", "calc.exe"]) is False


def test_validate_rejects_empty():
    assert validate_argv([]) is False


# ============== build_safe_argv ==============

def test_build_safe_argv_mail():
    argv = build_safe_argv("mail", mail_limit=50)
    assert argv == ["mail", "list", "--unread", "--limit", "50", "--yaml"]
    assert validate_argv(argv) is True


def test_build_safe_argv_shuiyuan():
    argv = build_safe_argv("shuiyuan", shuiyuan_limit=30)
    assert argv == ["shuiyuan", "latest", "--limit", "30", "--yaml"]
    assert validate_argv(argv) is True


def test_build_safe_argv_services():
    argv = build_safe_argv("services")
    assert argv == ["services", "pending", "--yaml"]


def test_build_safe_argv_messages():
    argv = build_safe_argv("messages")
    assert argv == ["messages", "list", "--unread-only", "--yaml"]


def test_build_safe_argv_card():
    argv = build_safe_argv("card")
    assert argv == ["card", "balance", "--yaml"]


def test_build_safe_argv_unknown_category_raises():
    with pytest.raises(SafetyViolation):
        build_safe_argv("library")  # library 不在 v1 范围


def test_build_safe_argv_rejects_oob_limit():
    """limit 必须正整数且 <= 200。"""
    with pytest.raises(SafetyViolation):
        build_safe_argv("mail", mail_limit=0)
    with pytest.raises(SafetyViolation):
        build_safe_argv("mail", mail_limit=201)
    with pytest.raises(SafetyViolation):
        build_safe_argv("mail", mail_limit=-5)
```

- [ ] **Step 2: 跑测试，确认全部 FAIL**

```powershell
pytest tests/test_safety.py -v
```

Expected: 全部 FAIL（`ModuleNotFoundError`）。

- [ ] **Step 3: 实现 `safety.py`**

文件 `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\safety.py`：

```python
"""命令白名单守门 —— v1 红线 2 / 3 / 4。

ALLOWED 是 v1 sjtu CLI 调用范围的完整封闭集合。任何不在白名单的
argv 都被拒绝，包括看似无害的额外参数（精确匹配，不放过任何提权
表面）。

加白名单的唯一姿势：
1. 改 ALLOWED 元组（必须是完整 argv，包含 --yaml）
2. 改 build_safe_argv 的 dispatch
3. 改 test_safety.py 的 test_allowed_is_exactly_N_commands + 测试相应分支
4. 在 PR 描述里说明加白理由 + 验证 sjtu CLI 该命令是只读
"""
from __future__ import annotations


class SafetyViolation(Exception):
    """命令不在白名单 / 参数非法。runner.py 不应 catch，直接 abort。"""


# v1 白名单：5 个 sjtu CLI 子命令的精确 argv。
# 每个元组的元素是 sjtu.exe 之后的参数（不含 sjtu 二进制本身）。
# 注：mail 和 shuiyuan 的 limit 是参数化的，validate_argv 单独处理。
ALLOWED: frozenset[tuple[str, ...]] = frozenset({
    ("services", "pending", "--yaml"),
    ("messages", "list", "--unread-only", "--yaml"),
    ("mail", "list", "--unread", "--limit", "<INT>", "--yaml"),       # <INT> 占位
    ("shuiyuan", "latest", "--limit", "<INT>", "--yaml"),
    ("card", "balance", "--yaml"),
})


def _replace_int_placeholder(argv: tuple[str, ...]) -> tuple[str, ...]:
    """把 <INT> 占位替换为占位本身，用于和外部 argv 模式匹配。"""
    return argv  # ALLOWED 本身就用 <INT> 占位


def validate_argv(argv: list[str]) -> bool:
    """精确匹配白名单。limit 字段允许任何 1..=200 的整数。"""
    if not argv:
        return False
    t = tuple(argv)
    # 直接精确匹配（services / messages / card）
    if t in ALLOWED:
        return True
    # mail / shuiyuan 模式：limit 位置换成 <INT> 后匹配
    for pattern in ALLOWED:
        if len(pattern) != len(t):
            continue
        # 找 <INT> 位置
        try:
            int_idx = pattern.index("<INT>")
        except ValueError:
            continue
        # int_idx 之外位置必须字面相等
        for i, (p, v) in enumerate(zip(pattern, t)):
            if i == int_idx:
                continue
            if p != v:
                break
        else:
            # int_idx 位置必须是 1..=200 整数字符串
            try:
                n = int(t[int_idx])
            except ValueError:
                return False
            if 1 <= n <= 200:
                return True
    return False


def build_safe_argv(
    category: str,
    *,
    mail_limit: int = 50,
    shuiyuan_limit: int = 30,
) -> list[str]:
    """构造 sjtu CLI argv（不含 sjtu.exe）。返回值经 validate_argv 验证。"""
    if category == "services":
        argv = ["services", "pending", "--yaml"]
    elif category == "messages":
        argv = ["messages", "list", "--unread-only", "--yaml"]
    elif category == "mail":
        if not (1 <= mail_limit <= 200):
            raise SafetyViolation(f"mail_limit 越界: {mail_limit}")
        argv = ["mail", "list", "--unread", "--limit", str(mail_limit), "--yaml"]
    elif category == "shuiyuan":
        if not (1 <= shuiyuan_limit <= 200):
            raise SafetyViolation(f"shuiyuan_limit 越界: {shuiyuan_limit}")
        argv = ["shuiyuan", "latest", "--limit", str(shuiyuan_limit), "--yaml"]
    elif category == "card":
        argv = ["card", "balance", "--yaml"]
    else:
        raise SafetyViolation(f"未知 category: {category}")

    # 自验证（防回归）
    if not validate_argv(argv):
        raise SafetyViolation(f"build 出的 argv 未通过 validate: {argv}")
    return argv
```

- [ ] **Step 4: 跑测试，确认全部 PASS**

```powershell
pytest tests/test_safety.py -v
```

Expected: 全部 PASS（约 20 个测试）。

- [ ] **Step 5: Commit**

```powershell
git add src/sjtu_daily/safety.py tests/test_safety.py
git commit -m "feat: safety.py 命令白名单守门（v1 红线 2/3/4）"
```

---

## Task 3: runner.py（subprocess + YAML 解析）

**Files:**
- Create: `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\runner.py`
- Create: `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\config.py`
- Create: `C:\Users\<your-username>\sjtu-daily\tests\test_runner.py`
- Create: `C:\Users\<your-username>\sjtu-daily\tests\test_config.py`
- Create: `C:\Users\<your-username>\sjtu-daily\tests\fixtures\envelope_mail_list.yaml`
- Create: `C:\Users\<your-username>\sjtu-daily\tests\fixtures\envelope_messages_list.yaml`
- Create: `C:\Users\<your-username>\sjtu-daily\tests\fixtures\envelope_services_pending.yaml`
- Create: `C:\Users\<your-username>\sjtu-daily\tests\fixtures\envelope_shuiyuan_latest.yaml`
- Create: `C:\Users\<your-username>\sjtu-daily\tests\fixtures\envelope_card_balance.yaml`
- Create: `C:\Users\<your-username>\sjtu-daily\tests\fixtures\envelope_error_session_expired.yaml`

- [ ] **Step 1: 写 fixture（envelope 样本）**

文件 `tests/fixtures/envelope_mail_list.yaml`：

```yaml
ok: true
schema_version: "1"
data:
  query: "is:unread in:inbox"
  count: 2
  offset: 0
  has_more: false
  items:
    - id: "12345"
      from_display: "Sender Name"
      from_address: "sender@example.com"
      subject: "测试通知 - fixture"
      fragment: "正文预览（这字段 runner 会 drop）"
      date_ms: 1716268800000
      size_bytes: 4096
      unread: true
    - id: "12346"
      subject: "另一封测试"
      date_ms: 1716265200000
      unread: true
```

文件 `tests/fixtures/envelope_messages_list.yaml`：

```yaml
ok: true
schema_version: "1"
data:
  page: 1
  unread_only: true
  returned: 1
  total: 1
  groups:
    - group_id: "ABC123"
      group_name: "教学秘书通知 fixture"
      unread_num: 2
      group_description: "..."
      is_group: true
      is_read: false
      create_time: "2026-05-21 08:00:00"
```

文件 `tests/fixtures/envelope_services_pending.yaml`：

```yaml
ok: true
schema_version: "1"
data:
  total: 1
  returned: 1
  with_identity: false
  my_applications:
    - id: "step-uuid-1"
      name: "填写申请"
      code: "ADD"
      assignTime: 1716268800
      process:
        id: "proc-uuid-1"
        name: "学位申请 fixture"
        entry: "20054472"
        update: 1716268900
        status: "doing"
        app:
          code: "HXBDSQ"
          name: "学位评定"
  awaiting_my_action: []
```

文件 `tests/fixtures/envelope_shuiyuan_latest.yaml`：

```yaml
ok: true
schema_version: "1"
data:
  page: 0
  returned: 1
  per_page: 30
  more_topics_url: null
  topics:
    - id: 123456
      title: "测试帖标题 fixture"
      fancy_title: "测试帖标题 fixture"
      posts_count: 5
      reply_count: 4
      views: 100
      like_count: 8
      last_posted_at: "2026-05-21T08:00:00.000Z"
      excerpt: "（runner 会 drop）"
      tags: []
```

文件 `tests/fixtures/envelope_card_balance.yaml`：

```yaml
ok: true
schema_version: "1"
data:
  card_no_redacted: "0012***"
  balance: "123.45"
  trans_balance: "0.00"
  expire_date: "2027-09-01"
  lost: false
  frozen: false
  face_type: "本科生"
  from_cache: false
  elapsed_ms: 1234
meta:
  via: "weixin"
  source_hint: "weixin.sjtu.edu.cn"
```

文件 `tests/fixtures/envelope_error_session_expired.yaml`：

```yaml
ok: false
schema_version: "1"
error:
  code: "session-expired"
  message: "session 已过期，请重新登录"
```

- [ ] **Step 2: 写失败测试 `tests/test_config.py`**

```python
"""config 模块测试。"""
from pathlib import Path

import pytest

from sjtu_daily.config import Config, load_config


def test_load_config_reads_toml(tmp_path):
    cfg = tmp_path / "config.toml"
    cfg.write_text(
        '''
[sjtu_cli]
binary = "C:/path/to/sjtu.exe"
timeout_seconds = 60

[mail]
limit = 100

[shuiyuan]
limit = 20

[notify]
app_name = "Test"

[scheduler]
min_interval_hours = 6
''',
        encoding="utf-8",
    )
    c = load_config(cfg)
    assert c.sjtu_binary == "C:/path/to/sjtu.exe"
    assert c.timeout_seconds == 60
    assert c.mail_limit == 100
    assert c.shuiyuan_limit == 20
    assert c.notify_app_name == "Test"
    assert c.min_interval_hours == 6


def test_load_config_defaults_when_missing(tmp_path):
    """config.toml 不存在时返默认值（首次运行场景）。"""
    cfg = tmp_path / "missing.toml"
    c = load_config(cfg)
    assert c.timeout_seconds == 30
    assert c.mail_limit == 50
    assert c.shuiyuan_limit == 30
    assert c.min_interval_hours == 6
    assert c.notify_app_name == "SJTU Daily"
    assert c.sjtu_binary == ""  # 空字符串 = 从 PATH 找
```

- [ ] **Step 3: 实现 `config.py`**

文件 `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\config.py`：

```python
"""config.toml 读取。tomllib 是 Python 3.11+ 标准库。"""
from __future__ import annotations

import tomllib
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class Config:
    sjtu_binary: str
    timeout_seconds: int
    mail_limit: int
    shuiyuan_limit: int
    notify_app_name: str
    min_interval_hours: int


_DEFAULTS = Config(
    sjtu_binary="",
    timeout_seconds=30,
    mail_limit=50,
    shuiyuan_limit=30,
    notify_app_name="SJTU Daily",
    min_interval_hours=6,
)


def load_config(path: Path) -> Config:
    """从 path 读 toml；不存在则返默认值。"""
    if not path.is_file():
        return _DEFAULTS
    with path.open("rb") as f:
        raw = tomllib.load(f)
    sjtu_cli = raw.get("sjtu_cli", {})
    mail = raw.get("mail", {})
    shuiyuan = raw.get("shuiyuan", {})
    notify = raw.get("notify", {})
    scheduler = raw.get("scheduler", {})
    return Config(
        sjtu_binary=sjtu_cli.get("binary", _DEFAULTS.sjtu_binary),
        timeout_seconds=int(sjtu_cli.get("timeout_seconds", _DEFAULTS.timeout_seconds)),
        mail_limit=int(mail.get("limit", _DEFAULTS.mail_limit)),
        shuiyuan_limit=int(shuiyuan.get("limit", _DEFAULTS.shuiyuan_limit)),
        notify_app_name=notify.get("app_name", _DEFAULTS.notify_app_name),
        min_interval_hours=int(scheduler.get("min_interval_hours", _DEFAULTS.min_interval_hours)),
    )
```

- [ ] **Step 4: 跑 config 测试，确认 PASS**

```powershell
pytest tests/test_config.py -v
```

Expected: 2 passed。

- [ ] **Step 5: 写失败测试 `tests/test_runner.py`**

```python
"""runner 模块测试 —— subprocess 走 mock，YAML 解析走真 fixture。"""
import subprocess
from decimal import Decimal
from pathlib import Path
from unittest.mock import MagicMock

import pytest

from sjtu_daily.config import Config
from sjtu_daily.runner import (
    CategoryResult,
    Snapshot,
    parse_card,
    parse_mail,
    parse_messages,
    parse_services,
    parse_shuiyuan,
    run_all,
    run_one,
)
from sjtu_daily.safety import SafetyViolation

FIXTURES = Path(__file__).parent / "fixtures"


def _cfg() -> Config:
    return Config(
        sjtu_binary="sjtu.exe",
        timeout_seconds=30,
        mail_limit=50,
        shuiyuan_limit=30,
        notify_app_name="Test",
        min_interval_hours=6,
    )


# ============== parsers（纯字符串 → dict 转换，无 subprocess）==============

def test_parse_mail_drops_pii():
    """red line 6: from_address / fragment / size_bytes 必须 drop。"""
    yaml_str = (FIXTURES / "envelope_mail_list.yaml").read_text(encoding="utf-8")
    items = parse_mail(yaml_str)
    assert len(items) == 2
    first = items[0]
    assert first["id"] == "12345"
    assert first["subject"] == "测试通知 - fixture"
    assert first["date_ms"] == 1716268800000
    assert first["unread"] is True
    # PII drop 守门
    assert "from_address" not in first
    assert "from_display" not in first
    assert "fragment" not in first
    assert "size_bytes" not in first


def test_parse_messages_keeps_only_metadata():
    yaml_str = (FIXTURES / "envelope_messages_list.yaml").read_text(encoding="utf-8")
    items = parse_messages(yaml_str)
    assert len(items) == 1
    g = items[0]
    assert g["id"] == "ABC123"
    assert g["title"] == "教学秘书通知 fixture"
    assert g["unread_num"] == 2
    assert g["create_time"] == "2026-05-21 08:00:00"


def test_parse_services_combines_two_lists():
    yaml_str = (FIXTURES / "envelope_services_pending.yaml").read_text(encoding="utf-8")
    items = parse_services(yaml_str)
    assert len(items) == 1
    s = items[0]
    assert s["id"] == "step-uuid-1"
    assert s["title"] == "学位申请 fixture"
    assert s["bucket"] == "my_applications"
    assert s["step_name"] == "填写申请"
    assert s["app_name"] == "学位评定"
    # owner / process.owner.* 字段 PII 绝不出
    assert "owner" not in s


def test_parse_services_handles_missing_process():
    """process 字段可能整个缺失，要宽松。"""
    yaml_str = """
ok: true
data:
  total: 1
  returned: 1
  my_applications:
    - id: "step-1"
      name: "step"
      code: "ADD"
  awaiting_my_action: []
"""
    items = parse_services(yaml_str)
    assert len(items) == 1
    assert items[0]["title"] == "step"  # 退回 step name


def test_parse_shuiyuan_drops_excerpt():
    yaml_str = (FIXTURES / "envelope_shuiyuan_latest.yaml").read_text(encoding="utf-8")
    items = parse_shuiyuan(yaml_str)
    assert len(items) == 1
    t = items[0]
    assert t["id"] == 123456
    assert t["title"] == "测试帖标题 fixture"
    assert t["last_posted_at"] == "2026-05-21T08:00:00.000Z"
    assert t["reply_count"] == 4
    assert "excerpt" not in t


def test_parse_card_returns_decimal():
    yaml_str = (FIXTURES / "envelope_card_balance.yaml").read_text(encoding="utf-8")
    bal = parse_card(yaml_str)
    assert bal["card_no_redacted"] == "0012***"
    assert bal["balance"] == Decimal("123.45")
    assert isinstance(bal["balance"], Decimal)
    assert bal["lost"] is False
    assert bal["frozen"] is False


def test_parse_card_never_uses_float():
    """red line 9: balance 字段不能进 float。"""
    yaml_str = (FIXTURES / "envelope_card_balance.yaml").read_text(encoding="utf-8")
    bal = parse_card(yaml_str)
    assert not isinstance(bal["balance"], float)


# ============== run_one（subprocess mock）==============

def test_run_one_success(mocker, tmp_path):
    fixture = (FIXTURES / "envelope_mail_list.yaml").read_text(encoding="utf-8")
    fake_proc = MagicMock(returncode=0, stdout=fixture, stderr="")
    mocker.patch("subprocess.run", return_value=fake_proc)

    result = run_one("mail", _cfg())
    assert result.ok is True
    assert result.category == "mail"
    assert len(result.items) == 2
    assert result.error is None


def test_run_one_session_expired_detected_by_envelope(mocker):
    fixture = (FIXTURES / "envelope_error_session_expired.yaml").read_text(encoding="utf-8")
    fake_proc = MagicMock(returncode=1, stdout=fixture, stderr="")
    mocker.patch("subprocess.run", return_value=fake_proc)

    result = run_one("mail", _cfg())
    assert result.ok is False
    assert result.auth_required is True
    assert "session" in (result.error or "").lower()


def test_run_one_session_expired_detected_by_stderr(mocker):
    """envelope 不出（异常退出）时靠 stderr SessionExpired 关键字。"""
    fake_proc = MagicMock(
        returncode=2,
        stdout="",
        stderr="Error: SessionExpired (please run `sjtu login`)",
    )
    mocker.patch("subprocess.run", return_value=fake_proc)

    result = run_one("mail", _cfg())
    assert result.ok is False
    assert result.auth_required is True


def test_run_one_timeout(mocker):
    mocker.patch(
        "subprocess.run",
        side_effect=subprocess.TimeoutExpired(cmd="sjtu", timeout=30),
    )
    result = run_one("mail", _cfg())
    assert result.ok is False
    assert result.auth_required is False
    assert "timeout" in (result.error or "").lower()


def test_run_one_generic_failure(mocker):
    fake_proc = MagicMock(returncode=42, stdout="", stderr="connection refused")
    mocker.patch("subprocess.run", return_value=fake_proc)
    result = run_one("mail", _cfg())
    assert result.ok is False
    assert result.auth_required is False


def test_run_one_uses_safe_argv(mocker):
    """red line 2: subprocess.run 收到的 argv 必须经 build_safe_argv。"""
    fixture = (FIXTURES / "envelope_card_balance.yaml").read_text(encoding="utf-8")
    fake_proc = MagicMock(returncode=0, stdout=fixture, stderr="")
    run_mock = mocker.patch("subprocess.run", return_value=fake_proc)

    run_one("card", _cfg())
    call_args = run_mock.call_args
    argv = call_args.args[0] if call_args.args else call_args.kwargs["args"]
    # argv[0] 是 sjtu.exe 路径，argv[1:] 是子命令
    assert argv[1:] == ["card", "balance", "--yaml"]


def test_run_one_rejects_unknown_category():
    with pytest.raises(SafetyViolation):
        run_one("library", _cfg())


# ============== run_all（编排 5 个 category）==============

def test_run_all_returns_snapshot(mocker):
    def fake_call(category, cfg):
        return CategoryResult(category=category, ok=True, items=[], error=None, auth_required=False)
    mocker.patch("sjtu_daily.runner.run_one", side_effect=fake_call)

    snap = run_all(_cfg())
    assert isinstance(snap, Snapshot)
    assert set(snap.results.keys()) == {"services", "messages", "mail", "shuiyuan", "card"}
    assert snap.has_any_auth_required is False


def test_run_all_propagates_auth_required(mocker):
    def fake_call(category, cfg):
        return CategoryResult(
            category=category,
            ok=False,
            items=[],
            error="SessionExpired",
            auth_required=(category == "mail"),
        )
    mocker.patch("sjtu_daily.runner.run_one", side_effect=fake_call)

    snap = run_all(_cfg())
    assert snap.has_any_auth_required is True
```

- [ ] **Step 6: 跑测试，确认全部 FAIL**

```powershell
pytest tests/test_runner.py -v
```

Expected: 全部 FAIL（`ModuleNotFoundError`）。

- [ ] **Step 7: 实现 `runner.py`**

文件 `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\runner.py`：

```python
"""subprocess 调 sjtu CLI 5 个白名单命令 + YAML 解析。

红线：
- argv 必走 safety.build_safe_argv（红线 2/3/4）
- 解析后必丢 PII 字段（红线 6）
- card.balance 必 Decimal（红线 9）
- 失败时 auth_required 区分（session 过期 vs 其他错）
"""
from __future__ import annotations

import logging
import subprocess
from dataclasses import dataclass, field
from decimal import Decimal
from typing import Any

import yaml

from sjtu_daily.config import Config
from sjtu_daily.safety import build_safe_argv

log = logging.getLogger(__name__)

CATEGORIES: tuple[str, ...] = ("services", "messages", "mail", "shuiyuan", "card")


@dataclass(frozen=True)
class CategoryResult:
    category: str
    ok: bool
    items: list[dict[str, Any]] = field(default_factory=list)
    error: str | None = None
    auth_required: bool = False
    # card.balance 用单独字段，否则 items 复用为长度 1 的 list
    card_balance: dict[str, Any] | None = None


@dataclass(frozen=True)
class Snapshot:
    """一次 run 的 5 个 category 全部结果。"""
    results: dict[str, CategoryResult]

    @property
    def has_any_auth_required(self) -> bool:
        return any(r.auth_required for r in self.results.values())


# ============== parsers ==============


def _envelope(yaml_str: str) -> dict[str, Any]:
    data = yaml.safe_load(yaml_str)
    if not isinstance(data, dict):
        raise ValueError(f"envelope 不是 dict: {type(data)}")
    return data


def parse_mail(yaml_str: str) -> list[dict[str, Any]]:
    """mail list envelope → 仅保留 id / subject / date_ms / unread。"""
    env = _envelope(yaml_str)
    items = env.get("data", {}).get("items", []) or []
    out = []
    for it in items:
        out.append({
            "id": str(it.get("id", "")),
            "subject": it.get("subject"),
            "date_ms": it.get("date_ms"),
            "unread": bool(it.get("unread", False)),
        })
    return out


def parse_messages(yaml_str: str) -> list[dict[str, Any]]:
    """messages list envelope → id / title / unread_num / create_time。"""
    env = _envelope(yaml_str)
    groups = env.get("data", {}).get("groups", []) or []
    out = []
    for g in groups:
        out.append({
            "id": str(g.get("group_id", "")),
            "title": g.get("group_name", ""),
            "unread_num": int(g.get("unread_num", 0)),
            "create_time": g.get("create_time"),
        })
    return out


def parse_services(yaml_str: str) -> list[dict[str, Any]]:
    """services pending envelope → 两个 bucket 合并，每项 id/title/bucket。"""
    env = _envelope(yaml_str)
    data = env.get("data", {}) or {}
    out = []
    for bucket in ("my_applications", "awaiting_my_action"):
        for item in data.get(bucket, []) or []:
            process = item.get("process") or {}
            app = (process or {}).get("app") or {}
            title = (
                process.get("name")
                or item.get("name")
                or "(无标题)"
            )
            out.append({
                "id": str(item.get("id", "")),
                "title": title,
                "bucket": bucket,
                "step_name": item.get("name"),
                "app_name": app.get("name"),
                "assign_time": item.get("assignTime"),
            })
    return out


def parse_shuiyuan(yaml_str: str) -> list[dict[str, Any]]:
    """shuiyuan latest envelope → id / title / last_posted_at / reply_count。"""
    env = _envelope(yaml_str)
    topics = env.get("data", {}).get("topics", []) or []
    out = []
    for t in topics:
        out.append({
            "id": str(t.get("id", "")),
            "title": t.get("title", ""),
            "last_posted_at": t.get("last_posted_at"),
            "reply_count": int(t.get("reply_count", 0)),
        })
    return out


def parse_card(yaml_str: str) -> dict[str, Any]:
    """card balance envelope → balance 强制 Decimal（红线 9）。"""
    env = _envelope(yaml_str)
    data = env.get("data", {}) or {}
    return {
        "card_no_redacted": data.get("card_no_redacted", ""),
        "balance": Decimal(str(data.get("balance", "0"))),
        "lost": bool(data.get("lost", False)),
        "frozen": bool(data.get("frozen", False)),
    }


# ============== subprocess runner ==============


def _looks_like_session_expired(stdout: str, stderr: str) -> bool:
    """envelope ok=false code=session-expired 或 stderr 含 SessionExpired。"""
    try:
        env = yaml.safe_load(stdout) if stdout else None
        if isinstance(env, dict) and env.get("ok") is False:
            code = (env.get("error") or {}).get("code", "")
            if "session" in code.lower():
                return True
    except yaml.YAMLError:
        pass
    if stderr and "SessionExpired" in stderr:
        return True
    return False


def _resolve_sjtu_binary(cfg: Config) -> str:
    if cfg.sjtu_binary:
        return cfg.sjtu_binary
    return "sjtu.exe"  # 从 PATH 找


def _parse_category(category: str, stdout: str) -> tuple[list[dict[str, Any]], dict[str, Any] | None]:
    if category == "mail":
        return parse_mail(stdout), None
    if category == "messages":
        return parse_messages(stdout), None
    if category == "services":
        return parse_services(stdout), None
    if category == "shuiyuan":
        return parse_shuiyuan(stdout), None
    if category == "card":
        return [], parse_card(stdout)
    raise ValueError(f"未知 category: {category}")


def run_one(category: str, cfg: Config) -> CategoryResult:
    """跑一个 category。所有失败收成 CategoryResult，不抛异常（除非 SafetyViolation）。"""
    argv = build_safe_argv(category, mail_limit=cfg.mail_limit, shuiyuan_limit=cfg.shuiyuan_limit)
    cmd = [_resolve_sjtu_binary(cfg), *argv]

    try:
        proc = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=cfg.timeout_seconds,
            check=False,
        )
    except subprocess.TimeoutExpired:
        return CategoryResult(
            category=category, ok=False, error=f"timeout after {cfg.timeout_seconds}s"
        )
    except FileNotFoundError as e:
        return CategoryResult(category=category, ok=False, error=f"sjtu 二进制找不到: {e}")

    if _looks_like_session_expired(proc.stdout, proc.stderr):
        return CategoryResult(
            category=category,
            ok=False,
            error="SessionExpired - 请运行 sjtu login",
            auth_required=True,
        )

    if proc.returncode != 0:
        return CategoryResult(
            category=category,
            ok=False,
            error=f"exit {proc.returncode}: {proc.stderr[:200]}",
        )

    try:
        items, card = _parse_category(category, proc.stdout)
    except (yaml.YAMLError, ValueError, KeyError, TypeError) as e:
        return CategoryResult(
            category=category, ok=False, error=f"parse 失败: {e}"
        )

    return CategoryResult(
        category=category, ok=True, items=items, card_balance=card
    )


def run_all(cfg: Config) -> Snapshot:
    """5 个 category 顺序跑（小心 throttle，并行容易撞 sjtu 子系统节流）。"""
    results: dict[str, CategoryResult] = {}
    for cat in CATEGORIES:
        log.info("running category=%s", cat)
        results[cat] = run_one(cat, cfg)
    return Snapshot(results=results)
```

- [ ] **Step 8: 跑测试，确认全部 PASS**

```powershell
pytest tests/test_runner.py tests/test_config.py -v
```

Expected: ~15 passed。

- [ ] **Step 9: Commit**

```powershell
git add src/sjtu_daily/runner.py src/sjtu_daily/config.py tests/test_runner.py tests/test_config.py tests/fixtures/
git commit -m "feat: runner + config + 5 子系统 envelope fixture + parser PII drop"
```

---

## Task 4: state.py（SQLite diff）

**Files:**
- Create: `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\state.py`
- Create: `C:\Users\<your-username>\sjtu-daily\tests\test_state.py`

- [ ] **Step 1: 写失败测试 `tests/test_state.py`**

```python
"""state 模块测试 —— SQLite schema 红线 + diff 正确性。"""
from datetime import datetime, timedelta, timezone
from pathlib import Path

import pytest

from sjtu_daily.state import StateDB


def test_schema_only_has_minimal_columns(tmp_path):
    """red line 5: 表只能有 (category, item_id, first_seen_at, notified_at, last_run_at_meta)，不能有 subject/body/from。"""
    db = StateDB(tmp_path / "state.db")
    db.init()
    cols = db.list_columns("seen")
    assert set(cols) == {"category", "item_id", "first_seen_at", "notified_at"}, (
        f"seen 表列出现 PII 字段: {cols}"
    )
    # meta 表只允许 (key, value) 两列
    meta_cols = db.list_columns("meta")
    assert set(meta_cols) == {"key", "value"}


def test_diff_new_items_first_run(tmp_path):
    """首次 run：全部 ID 都算新增。"""
    db = StateDB(tmp_path / "state.db")
    db.init()
    new_ids = db.diff_new_items("mail", ["a", "b", "c"])
    assert set(new_ids) == {"a", "b", "c"}


def test_diff_new_items_second_run(tmp_path):
    db = StateDB(tmp_path / "state.db")
    db.init()
    db.mark_seen("mail", ["a", "b"])
    new_ids = db.diff_new_items("mail", ["a", "b", "c"])
    assert new_ids == ["c"]


def test_diff_across_categories_isolated(tmp_path):
    """mail.a 和 messages.a 是不同条目。"""
    db = StateDB(tmp_path / "state.db")
    db.init()
    db.mark_seen("mail", ["a"])
    assert db.diff_new_items("messages", ["a"]) == ["a"]


def test_mark_seen_idempotent(tmp_path):
    db = StateDB(tmp_path / "state.db")
    db.init()
    db.mark_seen("mail", ["a"])
    db.mark_seen("mail", ["a"])  # 不应崩
    assert db.diff_new_items("mail", ["a"]) == []


def test_last_run_at_initially_none(tmp_path):
    db = StateDB(tmp_path / "state.db")
    db.init()
    assert db.last_run_at() is None


def test_last_run_at_after_record(tmp_path):
    db = StateDB(tmp_path / "state.db")
    db.init()
    now = datetime(2026, 5, 21, 7, 0, 0, tzinfo=timezone.utc)
    db.record_run_at(now)
    last = db.last_run_at()
    assert last == now


def test_should_skip_due_to_min_interval(tmp_path):
    """距离上次 run < min_interval 时 should_skip True。"""
    db = StateDB(tmp_path / "state.db")
    db.init()
    now = datetime(2026, 5, 21, 7, 0, 0, tzinfo=timezone.utc)
    db.record_run_at(now)
    # 2 小时后，min_interval=6h → skip
    assert db.should_skip_due_to_interval(
        now + timedelta(hours=2), min_interval_hours=6
    ) is True
    # 8 小时后 → not skip
    assert db.should_skip_due_to_interval(
        now + timedelta(hours=8), min_interval_hours=6
    ) is False


def test_should_skip_first_run_never(tmp_path):
    """从未跑过 → 必跑。"""
    db = StateDB(tmp_path / "state.db")
    db.init()
    now = datetime(2026, 5, 21, 7, 0, 0, tzinfo=timezone.utc)
    assert db.should_skip_due_to_interval(now, min_interval_hours=6) is False


def test_db_rejects_extra_columns(tmp_path):
    """守门：如果有人手贱加了 subject 列，init 时要拒。"""
    db_path = tmp_path / "state.db"
    # 人为造一个含 PII 列的表
    import sqlite3
    con = sqlite3.connect(db_path)
    con.execute(
        "CREATE TABLE seen (category TEXT, item_id TEXT, subject TEXT, "
        "first_seen_at TEXT, notified_at TEXT, PRIMARY KEY (category, item_id))"
    )
    con.commit()
    con.close()

    db = StateDB(db_path)
    with pytest.raises(RuntimeError, match="PII"):
        db.init()
```

- [ ] **Step 2: 跑测试，确认全部 FAIL**

```powershell
pytest tests/test_state.py -v
```

Expected: 全部 FAIL（`ModuleNotFoundError`）。

- [ ] **Step 3: 实现 `state.py`**

文件 `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\state.py`：

```python
"""SQLite 状态管理。

Schema 红线（红线 5）：
- seen 表只允许 4 列：(category, item_id, first_seen_at, notified_at)
- meta 表只允许 2 列：(key, value)
- 绝不允许 subject/body/from_*/title/fragment/excerpt 等 PII 列

init() 启动时校验现有 schema 是否含禁列；含则 raise。
"""
from __future__ import annotations

import sqlite3
from datetime import datetime, timezone
from pathlib import Path

_ALLOWED_SEEN_COLUMNS = frozenset({"category", "item_id", "first_seen_at", "notified_at"})
_ALLOWED_META_COLUMNS = frozenset({"key", "value"})

_SCHEMA_SEEN = """
CREATE TABLE IF NOT EXISTS seen (
    category TEXT NOT NULL,
    item_id TEXT NOT NULL,
    first_seen_at TEXT NOT NULL,
    notified_at TEXT,
    PRIMARY KEY (category, item_id)
);
"""

_SCHEMA_META = """
CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT
);
"""


class StateDB:
    def __init__(self, path: Path) -> None:
        self.path = path

    def _connect(self) -> sqlite3.Connection:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        con = sqlite3.connect(self.path)
        con.execute("PRAGMA foreign_keys = ON")
        return con

    def list_columns(self, table: str) -> list[str]:
        with self._connect() as con:
            cur = con.execute(f"PRAGMA table_info({table})")
            return [row[1] for row in cur.fetchall()]

    def init(self) -> None:
        """建表（若不存在）+ 守门校验 schema。"""
        with self._connect() as con:
            con.executescript(_SCHEMA_SEEN + _SCHEMA_META)
        # 校验
        seen_cols = set(self.list_columns("seen"))
        if seen_cols != _ALLOWED_SEEN_COLUMNS:
            extra = seen_cols - _ALLOWED_SEEN_COLUMNS
            raise RuntimeError(
                f"seen 表含禁列（PII 红线 5）: {extra}。"
                f"删除 {self.path} 后重建。"
            )
        meta_cols = set(self.list_columns("meta"))
        if meta_cols != _ALLOWED_META_COLUMNS:
            extra = meta_cols - _ALLOWED_META_COLUMNS
            raise RuntimeError(f"meta 表含禁列: {extra}")

    def diff_new_items(self, category: str, current_ids: list[str]) -> list[str]:
        """返回 current_ids 中没在 seen 表里的（即新增项）。"""
        if not current_ids:
            return []
        with self._connect() as con:
            placeholders = ",".join("?" * len(current_ids))
            cur = con.execute(
                f"SELECT item_id FROM seen WHERE category = ? AND item_id IN ({placeholders})",
                [category, *current_ids],
            )
            seen = {row[0] for row in cur.fetchall()}
        return [i for i in current_ids if i not in seen]

    def mark_seen(self, category: str, item_ids: list[str]) -> None:
        if not item_ids:
            return
        now = datetime.now(timezone.utc).isoformat()
        with self._connect() as con:
            con.executemany(
                "INSERT OR IGNORE INTO seen (category, item_id, first_seen_at) VALUES (?, ?, ?)",
                [(category, i, now) for i in item_ids],
            )
            con.commit()

    def record_run_at(self, t: datetime) -> None:
        with self._connect() as con:
            con.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('last_run_at', ?)",
                [t.isoformat()],
            )
            con.commit()

    def last_run_at(self) -> datetime | None:
        with self._connect() as con:
            cur = con.execute("SELECT value FROM meta WHERE key = 'last_run_at'")
            row = cur.fetchone()
        if not row:
            return None
        return datetime.fromisoformat(row[0])

    def should_skip_due_to_interval(self, now: datetime, *, min_interval_hours: int) -> bool:
        """距离上次 run < min_interval_hours 时返 True（Task Scheduler 双触发去重）。"""
        last = self.last_run_at()
        if last is None:
            return False
        elapsed = now - last
        return elapsed.total_seconds() < min_interval_hours * 3600
```

- [ ] **Step 4: 跑测试，确认全部 PASS**

```powershell
pytest tests/test_state.py -v
```

Expected: 10 passed。

- [ ] **Step 5: Commit**

```powershell
git add src/sjtu_daily/state.py tests/test_state.py
git commit -m "feat: state.py SQLite + schema PII 守门 + last_run interval 判定"
```

---

## Task 5: render.py + dashboard 模板

**Files:**
- Create: `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\render.py`
- Create: `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\templates\dashboard.html.j2`
- Create: `C:\Users\<your-username>\sjtu-daily\tests\test_render.py`

- [ ] **Step 1: 写失败测试 `tests/test_render.py`**

```python
"""render 模块测试 —— HTML 渲染 + PII 防漏。"""
from decimal import Decimal

from sjtu_daily.render import render_dashboard
from sjtu_daily.runner import CategoryResult, Snapshot


def _make_snapshot() -> Snapshot:
    return Snapshot(results={
        "mail": CategoryResult(
            category="mail",
            ok=True,
            items=[
                {"id": "M1", "subject": "测试邮件", "date_ms": 1716268800000, "unread": True},
            ],
        ),
        "messages": CategoryResult(
            category="messages",
            ok=True,
            items=[
                {"id": "G1", "title": "教学秘书通知", "unread_num": 2, "create_time": "2026-05-21 08:00:00"},
            ],
        ),
        "services": CategoryResult(category="services", ok=True, items=[]),
        "shuiyuan": CategoryResult(
            category="shuiyuan",
            ok=True,
            items=[
                {"id": "T1", "title": "水源测试帖", "last_posted_at": "2026-05-21T08:00:00Z", "reply_count": 3},
            ],
        ),
        "card": CategoryResult(
            category="card",
            ok=True,
            items=[],
            card_balance={"card_no_redacted": "0012***", "balance": Decimal("123.45"), "lost": False, "frozen": False},
        ),
    })


def test_render_contains_titles():
    snap = _make_snapshot()
    new_ids = {"mail": {"M1"}, "messages": set(), "services": set(), "shuiyuan": set(), "card": set()}
    html = render_dashboard(snap, new_ids)
    assert "测试邮件" in html
    assert "教学秘书通知" in html
    assert "水源测试帖" in html


def test_render_balance_with_two_decimals():
    snap = _make_snapshot()
    new_ids = {k: set() for k in ["mail", "messages", "services", "shuiyuan", "card"]}
    html = render_dashboard(snap, new_ids)
    assert "123.45" in html
    assert "0012***" in html


def test_render_marks_new_items():
    snap = _make_snapshot()
    new_ids = {"mail": {"M1"}, "messages": set(), "services": set(), "shuiyuan": set(), "card": set()}
    html = render_dashboard(snap, new_ids)
    # 新增项要有标记（class 或 emoji "🆕"）
    assert ("new-item" in html) or ("🆕" in html) or ("NEW" in html)


def test_render_shows_auth_required():
    snap = Snapshot(results={
        "mail": CategoryResult(
            category="mail", ok=False, error="SessionExpired", auth_required=True
        ),
        "messages": CategoryResult(category="messages", ok=True, items=[]),
        "services": CategoryResult(category="services", ok=True, items=[]),
        "shuiyuan": CategoryResult(category="shuiyuan", ok=True, items=[]),
        "card": CategoryResult(category="card", ok=True, items=[], card_balance=None),
    })
    new_ids = {k: set() for k in ["mail", "messages", "services", "shuiyuan", "card"]}
    html = render_dashboard(snap, new_ids)
    assert "sjtu login" in html
    assert "session" in html.lower() or "过期" in html


def test_render_does_not_leak_pii():
    """red line 6: 渲染绝不能出 from_address / from_display / fragment / excerpt。"""
    # 故意把 PII 塞进 items（应该被 render drop / 不引用）
    snap = Snapshot(results={
        "mail": CategoryResult(
            category="mail",
            ok=True,
            items=[
                {
                    "id": "M1",
                    "subject": "测试",
                    "date_ms": 1716268800000,
                    "unread": True,
                    # 这些字段不应该出现在 HTML
                    "from_address": "secret@example.com",
                    "fragment": "正文片段绝密",
                },
            ],
        ),
        "messages": CategoryResult(category="messages", ok=True, items=[]),
        "services": CategoryResult(category="services", ok=True, items=[]),
        "shuiyuan": CategoryResult(category="shuiyuan", ok=True, items=[]),
        "card": CategoryResult(category="card", ok=True, items=[], card_balance=None),
    })
    new_ids = {k: set() for k in ["mail", "messages", "services", "shuiyuan", "card"]}
    html = render_dashboard(snap, new_ids)
    assert "secret@example.com" not in html
    assert "正文片段绝密" not in html


def test_render_empty_category_shows_zero():
    """空 category 显示 "0 条"。"""
    snap = Snapshot(results={
        "mail": CategoryResult(category="mail", ok=True, items=[]),
        "messages": CategoryResult(category="messages", ok=True, items=[]),
        "services": CategoryResult(category="services", ok=True, items=[]),
        "shuiyuan": CategoryResult(category="shuiyuan", ok=True, items=[]),
        "card": CategoryResult(category="card", ok=True, items=[], card_balance=None),
    })
    new_ids = {k: set() for k in ["mail", "messages", "services", "shuiyuan", "card"]}
    html = render_dashboard(snap, new_ids)
    # 至少出现 5 次 0（每个 category 一次计数）
    assert html.count("0 条") >= 4 or html.count("(0)") >= 4
```

- [ ] **Step 2: 跑测试，确认全部 FAIL**

```powershell
pytest tests/test_render.py -v
```

Expected: 全部 FAIL（`ModuleNotFoundError`）。

- [ ] **Step 3: 写 Jinja2 模板**

文件 `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\templates\dashboard.html.j2`：

```html
<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<title>SJTU 今日 — {{ generated_at }}</title>
<style>
  body { font-family: -apple-system, "Microsoft YaHei", sans-serif; max-width: 880px; margin: 24px auto; padding: 0 16px; color: #222; }
  h1 { font-size: 22px; margin-bottom: 4px; }
  .meta { color: #888; font-size: 12px; margin-bottom: 28px; }
  section { margin-bottom: 24px; border: 1px solid #eee; border-radius: 8px; padding: 14px 18px; }
  section h2 { font-size: 16px; margin: 0 0 8px; }
  section h2 .count { color: #888; font-weight: normal; font-size: 13px; margin-left: 8px; }
  ul { list-style: none; padding-left: 0; margin: 8px 0 0; }
  li { padding: 6px 0; border-bottom: 1px dashed #f3f3f3; font-size: 14px; }
  li:last-child { border-bottom: none; }
  .new-item { background: #fff7e6; padding-left: 6px; }
  .new-tag { color: #d46b08; font-weight: bold; margin-right: 6px; }
  .time { color: #aaa; font-size: 12px; margin-left: 8px; }
  .auth-warning { background: #fff1f0; border-left: 4px solid #ff4d4f; padding: 8px 12px; margin: 8px 0; }
  .empty { color: #bbb; font-style: italic; padding: 4px 0; }
  .balance { font-size: 18px; font-weight: bold; color: #389e0d; }
</style>
</head>
<body>
<h1>SJTU 今日</h1>
<div class="meta">生成于 {{ generated_at }}{% if has_auth_required %} · <strong style="color:#ff4d4f">部分子系统 session 过期，请运行 <code>sjtu login</code></strong>{% endif %}</div>

{# ============== mail ============== #}
<section>
  <h2>📬 邮箱未读 <span class="count">({{ snap.mail.items|length }} 条)</span></h2>
  {% if snap.mail.auth_required %}
    <div class="auth-warning">session 过期 — 请运行 <code>sjtu login</code> 后重试</div>
  {% elif not snap.mail.ok %}
    <div class="auth-warning">查询失败：{{ snap.mail.error }}</div>
  {% elif snap.mail.items %}
    <ul>
      {% for m in snap.mail.items %}
        <li{% if m.id in new_ids.mail %} class="new-item"{% endif %}>
          {% if m.id in new_ids.mail %}<span class="new-tag">🆕</span>{% endif %}
          {{ m.subject or "(无主题)" }}
          {% if m.date_ms %}<span class="time">{{ m.date_ms | datems }}</span>{% endif %}
        </li>
      {% endfor %}
    </ul>
  {% else %}
    <div class="empty">0 条</div>
  {% endif %}
</section>

{# ============== messages ============== #}
<section>
  <h2>📨 交我办消息 <span class="count">({{ snap.messages.items|length }} 个未读分组)</span></h2>
  {% if snap.messages.auth_required %}
    <div class="auth-warning">session 过期 — 请运行 <code>sjtu login</code></div>
  {% elif not snap.messages.ok %}
    <div class="auth-warning">查询失败：{{ snap.messages.error }}</div>
  {% elif snap.messages.items %}
    <ul>
      {% for g in snap.messages.items %}
        <li{% if g.id in new_ids.messages %} class="new-item"{% endif %}>
          {% if g.id in new_ids.messages %}<span class="new-tag">🆕</span>{% endif %}
          {{ g.title }} <span class="time">{{ g.unread_num }} 条未读</span>
        </li>
      {% endfor %}
    </ul>
  {% else %}
    <div class="empty">0 条</div>
  {% endif %}
</section>

{# ============== services ============== #}
<section>
  <h2>📋 办事大厅待办 <span class="count">({{ snap.services.items|length }} 条)</span></h2>
  {% if snap.services.auth_required %}
    <div class="auth-warning">session 过期 — 请运行 <code>sjtu login</code></div>
  {% elif not snap.services.ok %}
    <div class="auth-warning">查询失败：{{ snap.services.error }}</div>
  {% elif snap.services.items %}
    <ul>
      {% for s in snap.services.items %}
        <li{% if s.id in new_ids.services %} class="new-item"{% endif %}>
          {% if s.id in new_ids.services %}<span class="new-tag">🆕</span>{% endif %}
          {{ s.title }}
          {% if s.bucket == "my_applications" %}<span class="time">我申请的 · {{ s.step_name or "" }}</span>
          {% else %}<span class="time">等我处理 · {{ s.step_name or "" }}</span>{% endif %}
        </li>
      {% endfor %}
    </ul>
  {% else %}
    <div class="empty">0 条</div>
  {% endif %}
</section>

{# ============== shuiyuan ============== #}
<section>
  <h2>💧 水源最新 <span class="count">({{ snap.shuiyuan.items|length }} 帖)</span></h2>
  {% if snap.shuiyuan.auth_required %}
    <div class="auth-warning">session 过期 — 请运行 <code>sjtu login</code></div>
  {% elif not snap.shuiyuan.ok %}
    <div class="auth-warning">查询失败：{{ snap.shuiyuan.error }}</div>
  {% elif snap.shuiyuan.items %}
    <ul>
      {% for t in snap.shuiyuan.items %}
        <li{% if t.id in new_ids.shuiyuan %} class="new-item"{% endif %}>
          {% if t.id in new_ids.shuiyuan %}<span class="new-tag">🆕</span>{% endif %}
          {{ t.title }}
          <span class="time">{{ t.reply_count }} 回复 · {{ t.last_posted_at or "" }}</span>
        </li>
      {% endfor %}
    </ul>
  {% else %}
    <div class="empty">0 条</div>
  {% endif %}
</section>

{# ============== card ============== #}
<section>
  <h2>💳 一卡通 <span class="count">{{ snap.card.card_balance.card_no_redacted if snap.card.card_balance else "" }}</span></h2>
  {% if snap.card.auth_required %}
    <div class="auth-warning">session 过期 — 请运行 <code>sjtu login</code></div>
  {% elif not snap.card.ok %}
    <div class="auth-warning">查询失败：{{ snap.card.error }}</div>
  {% elif snap.card.card_balance %}
    <div class="balance">¥{{ "%.2f"|format(snap.card.card_balance.balance) }}</div>
    {% if snap.card.card_balance.lost %}<div class="auth-warning">⚠️ 卡片已挂失</div>{% endif %}
    {% if snap.card.card_balance.frozen %}<div class="auth-warning">⚠️ 卡片已冻结</div>{% endif %}
  {% else %}
    <div class="empty">无数据</div>
  {% endif %}
</section>

</body>
</html>
```

- [ ] **Step 4: 实现 `render.py`**

文件 `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\render.py`：

```python
"""Jinja2 → dashboard.html。

red line 6：模板只引用 (id, subject/title, date_ms/last_posted_at,
reply_count/unread_num, card_no_redacted, balance, lost, frozen) 字段，
绝不引用 from_*, fragment, excerpt, body_*, owner.* 等 PII。
"""
from __future__ import annotations

from datetime import datetime, timezone
from pathlib import Path

from jinja2 import Environment, FileSystemLoader, select_autoescape

from sjtu_daily.runner import Snapshot


_TEMPLATE_DIR = Path(__file__).parent / "templates"


def _datems_filter(ms: int | None) -> str:
    if not ms:
        return ""
    dt = datetime.fromtimestamp(ms / 1000, tz=timezone.utc).astimezone()
    return dt.strftime("%Y-%m-%d %H:%M")


def _env() -> Environment:
    env = Environment(
        loader=FileSystemLoader(_TEMPLATE_DIR),
        autoescape=select_autoescape(["html", "j2"]),
    )
    env.filters["datems"] = _datems_filter
    return env


def render_dashboard(
    snap: Snapshot, new_ids: dict[str, set[str]], *, now: datetime | None = None
) -> str:
    """渲染 dashboard.html 内容。new_ids: 每个 category 中标"新增"的 item_id 集合。"""
    env = _env()
    tmpl = env.get_template("dashboard.html.j2")
    return tmpl.render(
        snap=snap.results,
        new_ids=new_ids,
        generated_at=(now or datetime.now()).strftime("%Y-%m-%d %H:%M:%S"),
        has_auth_required=snap.has_any_auth_required,
    )
```

- [ ] **Step 5: 跑测试，确认全部 PASS**

```powershell
pytest tests/test_render.py -v
```

Expected: 7 passed。

- [ ] **Step 6: Commit**

```powershell
git add src/sjtu_daily/render.py src/sjtu_daily/templates/dashboard.html.j2 tests/test_render.py
git commit -m "feat: Jinja2 dashboard 模板 + render.py + PII drop 守门"
```

---

## Task 6: notify.py（Windows Toast）

**Files:**
- Create: `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\notify.py`
- Create: `C:\Users\<your-username>\sjtu-daily\tests\test_notify.py`

- [ ] **Step 1: 写失败测试 `tests/test_notify.py`**

```python
"""notify 模块测试 —— Toast 包装 + 容错。"""
import pytest

from sjtu_daily.notify import send_summary_toast


def test_send_toast_no_crash_when_no_new(mocker):
    """0 新增 / 0 待办时不该发 Toast（避免每天弹无意义通知）。"""
    fake_toaster = mocker.MagicMock()
    mocker.patch("sjtu_daily.notify._make_toaster", return_value=fake_toaster)

    sent = send_summary_toast(
        new_counts={"mail": 0, "messages": 0, "services": 0, "shuiyuan": 0, "card": 0},
        auth_required=False,
        dashboard_url="file:///C:/Users/<your-username>/sjtu-daily/dashboard.html",
        app_name="Test",
    )
    assert sent is False
    fake_toaster.show_toast.assert_not_called()


def test_send_toast_when_new(mocker):
    fake_toaster = mocker.MagicMock()
    mocker.patch("sjtu_daily.notify._make_toaster", return_value=fake_toaster)

    sent = send_summary_toast(
        new_counts={"mail": 2, "messages": 1, "services": 0, "shuiyuan": 0, "card": 0},
        auth_required=False,
        dashboard_url="file:///C:/Users/<your-username>/sjtu-daily/dashboard.html",
        app_name="Test",
    )
    assert sent is True
    fake_toaster.show_toast.assert_called_once()


def test_send_toast_when_auth_required_even_if_zero_new(mocker):
    fake_toaster = mocker.MagicMock()
    mocker.patch("sjtu_daily.notify._make_toaster", return_value=fake_toaster)

    sent = send_summary_toast(
        new_counts={k: 0 for k in ["mail", "messages", "services", "shuiyuan", "card"]},
        auth_required=True,
        dashboard_url="file:///C:/Users/<your-username>/sjtu-daily/dashboard.html",
        app_name="Test",
    )
    assert sent is True


def test_send_toast_swallows_exceptions(mocker):
    """Toast 失败（windows-toasts 没装 / 系统不支持）不能让主流程崩。"""
    mocker.patch(
        "sjtu_daily.notify._make_toaster",
        side_effect=RuntimeError("winrt missing"),
    )
    sent = send_summary_toast(
        new_counts={"mail": 2, "messages": 0, "services": 0, "shuiyuan": 0, "card": 0},
        auth_required=False,
        dashboard_url="file:///x",
        app_name="Test",
    )
    assert sent is False  # 失败时返 False，但不抛
```

- [ ] **Step 2: 跑测试，确认全部 FAIL**

```powershell
pytest tests/test_notify.py -v
```

Expected: 全部 FAIL（`ModuleNotFoundError`）。

- [ ] **Step 3: 实现 `notify.py`**

文件 `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\notify.py`：

```python
"""Windows Toast 包装。Toast 失败永不让主流程崩。"""
from __future__ import annotations

import logging

log = logging.getLogger(__name__)


def _make_toaster(app_name: str):
    """延迟 import：windows-toasts 仅 Windows 装；其他平台或装不上时 raise。"""
    from windows_toasts import WindowsToaster  # type: ignore[import]
    return WindowsToaster(app_name)


def _make_toast(title: str, body: str, dashboard_url: str):
    from windows_toasts import Toast, ToastButton  # type: ignore[import]
    toast = Toast()
    toast.text_fields = [title, body]
    toast.AddAction(ToastButton(content="打开 dashboard", arguments=dashboard_url))
    return toast


def send_summary_toast(
    *,
    new_counts: dict[str, int],
    auth_required: bool,
    dashboard_url: str,
    app_name: str,
) -> bool:
    """发摘要 Toast。返 True = 成功发送；False = 未发（无新增 / 失败）。"""
    total_new = sum(new_counts.values())
    if total_new == 0 and not auth_required:
        log.info("无新增项 + 无 auth 警告，跳过 Toast")
        return False

    if auth_required:
        title = "⚠️ SJTU session 过期"
        body = "请运行 `sjtu login` 后再跑 sjtu-daily"
    else:
        parts = []
        if new_counts.get("mail", 0):
            parts.append(f"邮件 {new_counts['mail']}")
        if new_counts.get("messages", 0):
            parts.append(f"消息 {new_counts['messages']}")
        if new_counts.get("services", 0):
            parts.append(f"待办 {new_counts['services']}")
        if new_counts.get("shuiyuan", 0):
            parts.append(f"水源 {new_counts['shuiyuan']}")
        title = f"SJTU 今日新增 {total_new} 条"
        body = " / ".join(parts) if parts else "查看详情"

    try:
        toaster = _make_toaster(app_name)
        toast = _make_toast(title, body, dashboard_url)
        toaster.show_toast(toast)
        return True
    except Exception as e:
        log.warning("Toast 发送失败（不阻塞主流程）: %s", e)
        return False
```

- [ ] **Step 4: 跑测试，确认全部 PASS**

```powershell
pytest tests/test_notify.py -v
```

Expected: 4 passed。

- [ ] **Step 5: Commit**

```powershell
git add src/sjtu_daily/notify.py tests/test_notify.py
git commit -m "feat: notify.py Windows Toast + 0 新增不发 + 失败 swallow"
```

---

## Task 7: cli.py + __main__.py（入口）

**Files:**
- Create: `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\cli.py`
- Create: `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\__main__.py`
- Create: `C:\Users\<your-username>\sjtu-daily\tests\test_cli.py`

- [ ] **Step 1: 写失败测试 `tests/test_cli.py`**

```python
"""cli 测试 —— argparse 入口 + run/dry-run 编排。"""
from datetime import datetime, timezone

import pytest

from sjtu_daily.cli import main
from sjtu_daily.runner import CategoryResult, Snapshot


def _good_snapshot():
    return Snapshot(results={
        "mail": CategoryResult(category="mail", ok=True, items=[
            {"id": "M1", "subject": "x", "date_ms": 1716268800000, "unread": True},
        ]),
        "messages": CategoryResult(category="messages", ok=True, items=[]),
        "services": CategoryResult(category="services", ok=True, items=[]),
        "shuiyuan": CategoryResult(category="shuiyuan", ok=True, items=[]),
        "card": CategoryResult(
            category="card", ok=True, items=[], card_balance={
                "card_no_redacted": "0012***",
                "balance": __import__("decimal").Decimal("10.00"),
                "lost": False, "frozen": False,
            },
        ),
    })


def test_main_version_exits_zero(capsys):
    with pytest.raises(SystemExit) as exc:
        main(["version"])
    assert exc.value.code == 0
    out = capsys.readouterr().out
    assert "0.1.0" in out


def test_main_run_writes_dashboard(tmp_path, mocker, monkeypatch):
    monkeypatch.setenv("SJTU_DAILY_HOME", str(tmp_path))
    mocker.patch("sjtu_daily.cli.run_all", return_value=_good_snapshot())
    mocker.patch("sjtu_daily.cli.send_summary_toast", return_value=True)

    rc = main(["run"])
    assert rc == 0
    assert (tmp_path / "dashboard.html").is_file()
    html = (tmp_path / "dashboard.html").read_text(encoding="utf-8")
    assert "SJTU 今日" in html


def test_main_dry_run_does_not_write_state(tmp_path, mocker, monkeypatch):
    monkeypatch.setenv("SJTU_DAILY_HOME", str(tmp_path))
    mocker.patch("sjtu_daily.cli.run_all", return_value=_good_snapshot())
    toast_mock = mocker.patch("sjtu_daily.cli.send_summary_toast")

    rc = main(["dry-run"])
    assert rc == 0
    # dry-run 不写 state.db（不 mark_seen / 不 record_run_at）
    db_path = tmp_path / "data" / "state.db"
    if db_path.exists():
        import sqlite3
        con = sqlite3.connect(db_path)
        seen_count = con.execute("SELECT COUNT(*) FROM seen").fetchone()[0]
        con.close()
        assert seen_count == 0
    # dry-run 不发 Toast
    toast_mock.assert_not_called()


def test_main_run_skips_when_interval_too_short(tmp_path, mocker, monkeypatch):
    """距上次 run < 6h 时 silent exit 0，不发 Toast。"""
    monkeypatch.setenv("SJTU_DAILY_HOME", str(tmp_path))
    # 先跑一次（写 last_run_at）
    mocker.patch("sjtu_daily.cli.run_all", return_value=_good_snapshot())
    toast_mock = mocker.patch("sjtu_daily.cli.send_summary_toast")
    main(["run"])
    toast_mock.reset_mock()
    # 立刻再跑（距离 0 秒）
    rc = main(["run"])
    assert rc == 0
    toast_mock.assert_not_called()


def test_main_run_force_bypasses_interval(tmp_path, mocker, monkeypatch):
    """--force 跳过 interval 检查。"""
    monkeypatch.setenv("SJTU_DAILY_HOME", str(tmp_path))
    mocker.patch("sjtu_daily.cli.run_all", return_value=_good_snapshot())
    toast_mock = mocker.patch("sjtu_daily.cli.send_summary_toast")
    main(["run"])
    toast_mock.reset_mock()
    main(["run", "--force"])
    # force 跑了第二次 → render + toast 都重跑
    toast_mock.assert_called_once()


def test_main_unknown_subcommand(capsys):
    with pytest.raises(SystemExit) as exc:
        main(["bogus"])
    assert exc.value.code != 0
```

- [ ] **Step 2: 跑测试，确认全部 FAIL**

```powershell
pytest tests/test_cli.py -v
```

Expected: 全部 FAIL（`ModuleNotFoundError`）。

- [ ] **Step 3: 实现 `cli.py`**

文件 `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\cli.py`：

```python
"""CLI 入口：sjtu-daily {run|dry-run|version}。"""
from __future__ import annotations

import argparse
import logging
import sys
from datetime import datetime, timezone

from sjtu_daily import __version__, paths
from sjtu_daily.config import load_config
from sjtu_daily.notify import send_summary_toast
from sjtu_daily.render import render_dashboard
from sjtu_daily.runner import CATEGORIES, run_all
from sjtu_daily.state import StateDB


log = logging.getLogger("sjtu_daily")


def _setup_logging() -> None:
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )


def _new_ids_per_category(snap, db: StateDB, *, persist: bool) -> dict[str, set[str]]:
    """对每个 category 算 diff；persist=True 时 mark_seen 落库。"""
    out: dict[str, set[str]] = {}
    for cat in CATEGORIES:
        res = snap.results[cat]
        if not res.ok:
            out[cat] = set()
            continue
        # card 用 card_balance 单独字段，没有 list
        if cat == "card":
            out[cat] = set()
            continue
        ids = [it["id"] for it in res.items if it.get("id")]
        new_ids = db.diff_new_items(cat, ids)
        out[cat] = set(new_ids)
        if persist and ids:
            db.mark_seen(cat, ids)
    return out


def _do_run(*, dry_run: bool, force: bool) -> int:
    _setup_logging()
    paths.ensure_data_dir()
    cfg = load_config(paths.config_path())
    db = StateDB(paths.db_path())
    db.init()

    now = datetime.now(timezone.utc)
    if not dry_run and not force:
        if db.should_skip_due_to_interval(now, min_interval_hours=cfg.min_interval_hours):
            log.info("距上次 run < %dh，silent exit 0", cfg.min_interval_hours)
            return 0

    snap = run_all(cfg)
    new_ids = _new_ids_per_category(snap, db, persist=not dry_run)

    html = render_dashboard(snap, new_ids)
    dashboard = paths.dashboard_path()
    if dry_run:
        sys.stdout.write(html)
        log.info("dry-run: html → stdout (%d bytes)", len(html))
    else:
        dashboard.write_text(html, encoding="utf-8")
        log.info("dashboard 写入 %s", dashboard)

    if not dry_run:
        db.record_run_at(now)
        new_counts = {cat: len(new_ids.get(cat, set())) for cat in CATEGORIES}
        send_summary_toast(
            new_counts=new_counts,
            auth_required=snap.has_any_auth_required,
            dashboard_url=f"file:///{dashboard.as_posix()}",
            app_name=cfg.notify_app_name,
        )
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="sjtu-daily")
    sub = parser.add_subparsers(dest="cmd", required=True)
    p_run = sub.add_parser("run", help="跑一次完整流程：拉数据 + 写 dashboard + Toast")
    p_run.add_argument("--force", action="store_true", help="跳过 min_interval 检查")
    sub.add_parser("dry-run", help="只跑 + 输出 html 到 stdout，不写 state / 不发 Toast")
    sub.add_parser("version", help="打印版本号")

    args = parser.parse_args(argv)

    if args.cmd == "version":
        print(__version__)
        raise SystemExit(0)
    if args.cmd == "run":
        return _do_run(dry_run=False, force=args.force)
    if args.cmd == "dry-run":
        return _do_run(dry_run=True, force=False)
    parser.print_help()
    return 2
```

- [ ] **Step 4: 实现 `__main__.py`**

文件 `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\__main__.py`：

```python
"""支持 python -m sjtu_daily ..."""
import sys

from sjtu_daily.cli import main

if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]) or 0)
```

- [ ] **Step 5: 跑测试，确认全部 PASS**

```powershell
pytest tests/test_cli.py -v
```

Expected: 6 passed。

- [ ] **Step 6: 跑全套测试**

```powershell
pytest -v
```

Expected: 全部 PASS（约 50+ 个测试）。

- [ ] **Step 7: Commit**

```powershell
git add src/sjtu_daily/cli.py src/sjtu_daily/__main__.py tests/test_cli.py
git commit -m "feat: cli.py 入口 + run/dry-run/version + min_interval skip + --force"
```

---

## Task 8: install-task.ps1（Windows Task Scheduler）

**Files:**
- Create: `C:\Users\<your-username>\sjtu-daily\scripts\install-task.ps1`
- Create: `C:\Users\<your-username>\sjtu-daily\scripts\uninstall-task.ps1`

- [ ] **Step 1: 写 install-task.ps1**

文件 `C:\Users\<your-username>\sjtu-daily\scripts\install-task.ps1`：

```powershell
# install-task.ps1 — 创建 Windows Task Scheduler 任务跑 sjtu-daily
#
# 触发器：
#   1. 每天 07:00 跑（如果电脑已开机）
#   2. 用户登录时跑（电脑关机一夜后，早上 8:30 才开机时补跑）
#
# wrapper（cli.py）内部 min_interval_hours=6 防同一天重跑。
#
# 红线：
#   - 任务只在 *当前用户* 登录时运行（不要 SYSTEM 账户，避免 Toast 不显示）
#   - 不要 -ExecutionPolicy Bypass 全局，只对本任务的 powershell.exe 调用降级

param(
    [string]$ProjectRoot = "$env:USERPROFILE\sjtu-daily",
    [string]$TaskName = "SJTU-Daily",
    [string]$DailyTime = "07:00"
)

$ErrorActionPreference = "Stop"

# 1. 找 venv 里的 python.exe
$python = Join-Path $ProjectRoot ".venv\Scripts\python.exe"
if (-not (Test-Path $python)) {
    Write-Error "未找到 venv：$python。请先在 $ProjectRoot 跑 python -m venv .venv && pip install -e ."
    exit 1
}

# 2. 删除旧任务（幂等）
$existing = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
if ($existing) {
    Write-Host "删除已有任务：$TaskName"
    Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
}

# 3. 构造 Action
$action = New-ScheduledTaskAction `
    -Execute $python `
    -Argument "-m sjtu_daily run" `
    -WorkingDirectory $ProjectRoot

# 4. 双触发器
$dailyTrigger = New-ScheduledTaskTrigger -Daily -At $DailyTime
$logonTrigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME

# 5. Settings：仅当用户登录时运行（Toast 才能弹）
$settings = New-ScheduledTaskSettingsSet `
    -StartWhenAvailable `
    -DontStopOnIdleEnd `
    -ExecutionTimeLimit (New-TimeSpan -Minutes 10) `
    -MultipleInstances IgnoreNew `
    -AllowStartIfOnBatteries `
    -DontStopIfGoingOnBatteries

# 6. Principal：当前用户 + 交互式（Toast 必须）
$principal = New-ScheduledTaskPrincipal `
    -UserId $env:USERNAME `
    -LogonType Interactive `
    -RunLevel Limited

# 7. 注册任务
Register-ScheduledTask `
    -TaskName $TaskName `
    -Action $action `
    -Trigger @($dailyTrigger, $logonTrigger) `
    -Settings $settings `
    -Principal $principal `
    -Description "sjtu-daily：每日 SJTU 子系统聚合 + 本地 HTML dashboard + Toast。每天 $DailyTime + 用户登录时触发；wrapper 内部去重防重跑。"

Write-Host "✅ 任务 $TaskName 已注册。"
Write-Host "   触发：每日 $DailyTime + 用户登录"
Write-Host "   命令：$python -m sjtu_daily run"
Write-Host ""
Write-Host "下一步："
Write-Host "  1. 复制 $ProjectRoot\config.example.toml → $ProjectRoot\config.toml 并按需修改"
Write-Host "  2. 在主目录跑一次 sjtu login（如未登录）"
Write-Host "  3. 立即跑一次：& '$python' -m sjtu_daily run --force"
```

- [ ] **Step 2: 写 uninstall-task.ps1**

文件 `C:\Users\<your-username>\sjtu-daily\scripts\uninstall-task.ps1`：

```powershell
param([string]$TaskName = "SJTU-Daily")

$existing = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
if ($existing) {
    Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
    Write-Host "✅ 任务 $TaskName 已删除"
} else {
    Write-Host "ℹ️  任务 $TaskName 不存在，无需删除"
}
```

- [ ] **Step 3: 手动验证（不能 TDD，但跑一次确认）**

```powershell
cd C:\Users\<your-username>\sjtu-daily
powershell -ExecutionPolicy Bypass -File .\scripts\install-task.ps1
Get-ScheduledTask -TaskName SJTU-Daily | Format-List TaskName, State, Triggers
```

Expected:
- TaskName: SJTU-Daily
- State: Ready
- Triggers: 两个（Daily @ 07:00 + AtLogOn）

然后立即触发一次：

```powershell
Start-ScheduledTask -TaskName SJTU-Daily
Start-Sleep 5
Get-ScheduledTaskInfo -TaskName SJTU-Daily | Format-List LastRunTime, LastTaskResult
```

Expected: LastTaskResult = 0。

- [ ] **Step 4: Commit**

```powershell
git add scripts/install-task.ps1 scripts/uninstall-task.ps1
git commit -m "feat: install-task.ps1 双触发器 + 仅交互登录时运行"
```

---

## Task 9: README + 端到端冒烟

**Files:**
- Create: `C:\Users\<your-username>\sjtu-daily\README.md`

- [ ] **Step 1: 写 README.md**

文件 `C:\Users\<your-username>\sjtu-daily\README.md`：

```markdown
# sjtu-daily

Windows 本地每日待办 dashboard —— 调既有 [SJTU-CLI](https://github.com/wuyutanhongyuxin-cell/SJTU_CLI) 拉 5 个子系统数据，生成本地 HTML + Windows Toast。

## 设计原则

- **零侵入 sjtu-cli**：subprocess 调既有二进制，不引用 Rust 源码
- **PII 永不落盘**：SQLite 只存 `(category, item_id, notified_at)`，HTML 只显示标题 + ID + 时间
- **命令白名单**：sjtu CLI 调用受 `safety.ALLOWED` 死锁，仅 5 个只读命令
- **失败显式**：session 过期 → Toast "请运行 sjtu login"，不静默吞
- **v1 不联网**：不调 Ollama / Gemini / Notion 等任何外部 API

## v1 范围

| Category | sjtu CLI 命令 | 用途 |
|---|---|---|
| mail | `mail list --unread --limit 50 --yaml` | 未读邮件标题 |
| messages | `messages list --unread-only --yaml` | 交我办未读消息分组 |
| services | `services pending --yaml` | 办事大厅待办 |
| shuiyuan | `shuiyuan latest --limit 30 --yaml` | 水源最新帖（仅标题） |
| card | `card balance --yaml` | 一卡通余额 |

**永不调**的命令（红线）：`messages show`（标已读副作用） / `card auth` / `mail read` / 任何写命令。

## 安装

前置：Python 3.11+ 已装 / SJTU-CLI 已 build 出 `sjtu.exe` / 已跑过 `sjtu login`。

```powershell
git clone https://github.com/<you>/sjtu-daily.git C:\Users\$env:USERNAME\sjtu-daily
cd C:\Users\$env:USERNAME\sjtu-daily
python -m venv .venv
.\.venv\Scripts\Activate.ps1
pip install -e .[dev]

# 配置（可选，不配置走默认）
Copy-Item config.example.toml config.toml
notepad config.toml  # 改 sjtu_cli.binary 指向你的 sjtu.exe

# 装 Task Scheduler 任务
powershell -ExecutionPolicy Bypass -File .\scripts\install-task.ps1

# 立即跑一次验证
.\.venv\Scripts\python -m sjtu_daily run --force
start C:\Users\$env:USERNAME\sjtu-daily\dashboard.html
```

## 命令

```powershell
sjtu-daily run            # 跑一次（受 min_interval_hours=6 限制）
sjtu-daily run --force    # 跳过 interval 检查
sjtu-daily dry-run        # 不写 state / 不发 Toast，html → stdout
sjtu-daily version
```

## 调度规则

Task Scheduler 双触发器：

1. 每天 07:00（电脑已开机时）
2. 用户登录时（电脑前一夜关机的话，早上开机时补跑）

wrapper 内部 `min_interval_hours=6` 防同一天重跑两次。改这个值见 `config.toml [scheduler] min_interval_hours`。

## session 过期

sjtu CLI 的 jaccount session 一般 30 天过期。过期时 sjtu-daily 会：

1. Toast 标题 "⚠️ SJTU session 过期"
2. dashboard.html 顶部红框警告
3. 该 category **不写入 seen 表**（下次 login 后能正确补推遗漏项）

用户操作：在主目录跑 `sjtu login` 重新扫码 → 下次 sjtu-daily 自动恢复。

## 卸载

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\uninstall-task.ps1
Remove-Item -Recurse -Force C:\Users\$env:USERNAME\sjtu-daily
```

## 测试

```powershell
.\.venv\Scripts\python -m pytest -v
```

## 许可

MIT
```

- [ ] **Step 2: 端到端冒烟（手测）**

前置确认：
1. `sjtu.exe` 已 build 在 `E:/claude_ask/sjtu_CLI/sjtu-cli/target/release/sjtu.exe`
2. 已跑过 `sjtu login`，`~/.sjtu-cli/session.json` 存在
3. config.toml 已配 `sjtu_cli.binary`

跑：

```powershell
cd C:\Users\<your-username>\sjtu-daily
.\.venv\Scripts\python -m sjtu_daily run --force
```

期望 stdout：

```
... INFO sjtu_daily: running category=services
... INFO sjtu_daily: running category=messages
... INFO sjtu_daily: running category=mail
... INFO sjtu_daily: running category=shuiyuan
... INFO sjtu_daily: running category=card
... INFO sjtu_daily: dashboard 写入 C:\Users\<your-username>\sjtu-daily\dashboard.html
```

然后：

```powershell
start C:\Users\<your-username>\sjtu-daily\dashboard.html
```

验收：
- [ ] dashboard 含 5 个 section（邮箱 / 消息 / 待办 / 水源 / 一卡通）
- [ ] 一卡通余额显示为 `¥123.45` 格式（两位小数）
- [ ] 首次跑：所有项都标 🆕（新增）
- [ ] 立即再跑 `sjtu-daily run --force`：之前的项不再标 🆕
- [ ] dashboard.html grep 不到 from_address / fragment / excerpt 等 PII 字段

```powershell
Select-String -Path C:\Users\<your-username>\sjtu-daily\dashboard.html -Pattern "from_address|fragment|excerpt|body_plain"
```

Expected: 无匹配。

- [ ] **Step 3: Commit**

```powershell
git add README.md
git commit -m "docs: README + 安装 + 调度 + session 过期 + 验收"
```

- [ ] **Step 4: 推到 GitHub**

```powershell
cd C:\Users\<your-username>\sjtu-daily
gh repo create sjtu-daily --private --source=. --remote=origin --push
```

或手动：

```powershell
git remote add origin https://github.com/wuyutanhongyuxin-cell/sjtu-daily.git
git push -u origin main
```

---

## 全局验收（v1 完工标志）

完工 = 全部 ✅：

- [ ] `pytest -v` 全绿（约 50+ 测试）
- [ ] `sjtu-daily run --force` 在干净 venv 上能跑出 dashboard.html
- [ ] dashboard.html 5 个 section 都显示（即使有空 category）
- [ ] grep dashboard.html 不到 PII 字段（from_address / fragment / excerpt / body_plain）
- [ ] grep state.db schema 只有 4 列（category / item_id / first_seen_at / notified_at）
- [ ] Task Scheduler 任务已注册，State=Ready，触发后 LastTaskResult=0
- [ ] Toast 在首次跑时弹出（含"打开 dashboard"按钮）
- [ ] 立即第二次跑（间隔 < 6h）：silent exit 0，无 Toast
- [ ] `sjtu-daily run --force` 强制绕过 interval：再次跑出 Toast，但所有项都不再 🆕（已 seen）
- [ ] 手动断网 / 改 config.binary 指向不存在路径 / 故意删 ~/.sjtu-cli/session.json：5 个 category 显示 auth_required 警告，主流程不崩
- [ ] sjtu-cli git 仓库 `git status` 干净（**零侵入**红线）

---

## v2 路线图（不在本 plan 范围，记在 README）

- **LLM 摘要**：本地 Ollama Qwen3 4B 对每个 category 出"一句话 TL;DR"
- **Notion 同步**：选配 cloud dashboard，仍只发 "标题 + ID"，不发 body
- **更多子系统**：library / canvas / elec / jwc grades 接入
- **Linux/Mac**：抽 notify backend，加 notify-send / osascript 后端
- **VPS 部署（方案 C）**：本地跑 sjtu CLI，Cloudflare Tunnel 推 JSON 到 VPS Hermes
