# sjtu-daily v1.1 LLM 摘要层实施 Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 给 sjtu-daily v1（每天 1 次跑 5 个 SJTU 子系统出 dashboard.html + Toast）加 LLM 摘要层 —— Gemini 2.5 Flash-Lite 出结构化 JSON 优先级分析 → DeepSeek-V4-Flash 接 JSON 产中文文案，dashboard 顶部渲染 "📌 今日重点 / ⚠️ 紧急 / 💡 建议" 三段 + Toast punchline 用 LLM 生成。

**Architecture:** 双 LLM cascade + 双向 fallback：(1) 正向：Gemini analyze（response_schema 强制 JSON）→ DeepSeek polish（中文文案润色）。(2) Fallback 1：Gemini 失败 → DeepSeek 单独跑全流程（analyze + polish 合并 prompt）。(3) Fallback 2：DeepSeek 失败 → Gemini 输出直接渲染（粗糙但能看）。(4) 双挂兜底：no-summary 模式，dashboard 顶部不渲染 summary 区块，cli 退出 0，Toast 走 v1 原逻辑。LLM 输出当 plaintext 处理，Jinja2 autoescape 防 HTML 注入。

**Tech Stack:** Python 3.11+ / `google-genai>=1.0.0`（旧 `google-generativeai` 2025-08-31 已弃用）/ `openai>=1.50.0`（DeepSeek 走 OpenAI 兼容，`base_url="https://api.deepseek.com"`）/ `pydantic>=2.0`（response_schema 校验 + LLMConfig）/ 现有 jinja2 / pyyaml / windows-toasts / pytest + pytest-mock

---

## 红线契约 8 条（implementer 必读，违者 NACK；每条都有守护测试）

1. **API key 永不入 git**：`config.toml` 已 ignore（v1 已配 `.gitignore`）；test_config 验 `.gitignore` 内含 `config.toml`；任何 log 输出 key 时脱敏（前 8 位 + `***`）。
2. **prompt 永不带 PII**：prompt 渲染前过白名单——mail 只发 `subject/date_ms/unread`；shuiyuan 只发 `title/last_posted_at/reply_count`；services 只发 `title/bucket/step_name/app_name/assign_time`；messages 只发 `title/unread_num/create_time`；card 只发 `balance/lost/frozen/card_no_redacted`。**永不发** `from_address` / `from_display` / `fragment` / `excerpt` / `body_plain` / 学号 / 姓名 / IP。test_prompts 跑全 fixture，assert 0 PII 字段串出现在 prompt 文本里。
3. **LLM 输出永不当 HTML 渲染**：LLM 输出当 plaintext；Jinja2 默认 autoescape；prompt 内明文要求 LLM "返回纯 plain text, no HTML, no markdown link syntax"。test_render 加 case：LLM 返回 `<script>alert(1)</script>` dashboard 不含可执行 HTML（验 `&lt;script&gt;`）。
4. **LLM 失败 ≠ dashboard 失败**：`summarize()` 抛 LLMError，cli.py 捕获记 log，summary=None 继续 render，exit 0。test_cli 加 case：mock summarize raise → render 仍出 + exit 0。
5. **超时硬限**：每次单 LLM call 默认 15s timeout（config 可调），整个 pipeline 累计预算 45s。pipeline.py 用 wall-clock 累计预算守门。
6. **成本可观测**：state.db `llm_runs` 表每次跑记一行（含 dry-run）；schema 严格 9 列，禁任何 PII 列（prompt/response 文本永不入库）。
7. **不缓存 LLM 输出到磁盘**：dashboard.html 是渲染产物不是缓存；state.db 只记 metadata 不记 prompt/output 内容。
8. **fallback gate**：Gemini JSON 不通过 pydantic `AnalysisResult` 校验 → 视作失败走 fallback（防 schema drift / 防 LLM 漂移注入额外字段）。

**v1 旧红线继续生效**：零侵入 sjtu-cli / .env 不入 git / data/ 不入 git / 单 .py ≤ 200 行 / 测试 ≤ 300 行 / 不缓存邮件正文 / Decimal 金额 / cookie 不离本机。

---

## Tradeoffs

- **双 LLM cascade vs 单 LLM**：选 cascade 因 Gemini 强 schema、DeepSeek 强中文润色；代价是延迟翻倍（10-20s）+ 失败面翻倍。fallback 把"延迟翻倍"摊到失败路径（happy path 也只是两段顺序 call，约 8-12s）。
- **response_schema 强 JSON vs 自由文本**：选强 JSON 因下游 polish 步骤需要稳定字段；代价是 schema drift 时整段 fallback。Gate 设计已覆盖。
- **plaintext vs markdown**：选 plaintext 因 Jinja2 autoescape 直接安全；代价是格式较朴素（无加粗 / 无链接）。markdown 引入 `bleach` 库 + allowlist 复杂度 ROI 低，v1.1 不做。
- **API key 在 config.toml 明文 vs keyring**：选 config.toml 明文因依赖最少（用户已习惯 sjtu-cli 的 session.json 明文）；config.toml 已 .gitignore + 文件权限默认 600（Win ACL 留 v1.2）；test_config 加 gitignore guard。
- **dry-run 时是否调 LLM**：选**调**（记成本 + 看效果）；test_cli 加 case 验 dry-run 走 LLM 路径但**不写 dashboard.html / 不发 Toast / 不 mark_seen**（继承 v1 dry-run 语义）。
- **--no-llm flag**：选**加**，离线 / debug / 省钱场景用；行为等同于 `cfg.llm.mode = "off"`。

---

## Out of scope (v1.2+)

- 本地 Ollama / vLLM 路线（v1.2 加 OllamaProvider 走同一 Protocol）
- Toast actionable buttons "查看邮件" / "打开水源帖"（v1.2 RPC 跳转）
- Notion / 飞书 / Slack 同步
- 多用户 / 多账号
- LLM 输出 markdown + bleach allowlist 渲染
- LLM 内容缓存（同样 snapshot 复用 LLM 输出）
- prompt 模板国际化（中文以外）
- Linux/Mac notify backend
- Windows ACL 加固 config.toml 权限

---

## 文件结构（v1.1 增量）

```
C:\Users\<your-username>\sjtu-daily\
├── pyproject.toml                            # 改：deps 加 google-genai / openai / pydantic
├── config.example.toml                       # 改：加 [llm] section
├── README.md                                 # 改：加 v1.1 章节
├── src/sjtu_daily/
│   ├── config.py                             # 改：Config 加 llm 字段
│   ├── state.py                              # 改：init() 加 llm_runs 表 + record_llm_run
│   ├── render.py                             # 改：签名加 summary 参数
│   ├── notify.py                             # 改：send_summary_toast 加 punchline
│   ├── cli.py                                # 改：_do_run 接 summarize + --no-llm
│   ├── templates/dashboard.html.j2           # 改：顶部加 summary section
│   └── llm/                                  # 新建子模块
│       ├── __init__.py                       # 对外暴露 summarize / SummaryResult / LLMError
│       ├── base.py                           # LLMProvider Protocol + SummaryResult + LLMError + RunMeta
│       ├── schemas.py                        # LLMConfig + AnalysisResult + PolishedResult (pydantic)
│       ├── prompts.py                        # 3 段 prompt + build_*_input 渲染函数
│       ├── gemini.py                         # GeminiProvider (google-genai)
│       ├── deepseek.py                       # DeepSeekProvider (openai SDK)
│       └── pipeline.py                       # summarize() 双向 fallback 编排
└── tests/llm/                                # 新建测试目录
    ├── __init__.py
    ├── test_base.py                          # Protocol 契约 / SummaryResult / LLMError
    ├── test_schemas.py                       # pydantic 校验 + 拒绝越权字段
    ├── test_prompts.py                       # 0 PII / 渲染含 5 section
    ├── test_gemini.py                        # mock google-genai
    ├── test_deepseek.py                      # mock openai client
    └── test_pipeline.py                      # 4 路径 + cost 记账
```

---

## 关键类型 / 签名速查（implementer 不要漂移，跨 task 一致）

```python
# llm/schemas.py
class LLMConfig(BaseModel):
    mode: Literal["off", "cascade", "gemini_only", "deepseek_only"] = "off"
    gemini_api_key: str = ""
    gemini_model: str = "gemini-2.5-flash-lite"
    deepseek_api_key: str = ""
    deepseek_model: str = "deepseek-v4-flash"
    timeout_seconds: int = 15
    pipeline_budget_seconds: int = 45
    fallback_on_error: bool = True
    max_tokens_out: int = 800

class AnalysisResult(BaseModel):
    urgent: list[str]          # 紧急事项（最多 5 条短句）
    today_highlights: list[str]  # 今日重点（最多 5 条）
    suggestions: list[str]     # 建议（最多 3 条）
    cross_cutting: list[str]   # 跨子系统观察（最多 3 条）

class PolishedResult(BaseModel):
    today_highlights_text: str   # 已润色"今日重点"段（plaintext, ≤300 字）
    urgent_text: str             # 已润色"紧急"段（plaintext, ≤200 字）
    suggestions_text: str        # 已润色"建议"段（plaintext, ≤200 字）
    punchline: str               # Toast body 用一句话（plaintext, ≤40 字）

# llm/base.py
@dataclass(frozen=True)
class RunMeta:
    provider: str            # "gemini" / "deepseek"
    model: str
    mode: str                # "analyze" / "polish" / "fallback_full"
    latency_ms: int
    tokens_in: int
    tokens_out: int
    cost_usd: float          # 估算（model 单价表内置）
    ok: bool
    error: str | None = None

@dataclass(frozen=True)
class SummaryResult:
    today_highlights_text: str
    urgent_text: str
    suggestions_text: str
    punchline: str
    runs: list[RunMeta]      # 本次 pipeline 所有 LLM call 元信息

class LLMError(Exception):
    def __init__(self, msg: str, *, provider: str, retryable: bool = False, cause: Exception | None = None): ...

class LLMProvider(Protocol):
    name: str
    def generate_json(self, prompt: str, *, schema: dict, timeout: int) -> tuple[dict, RunMeta]: ...
    def generate_text(self, prompt: str, *, max_tokens: int, timeout: int) -> tuple[str, RunMeta]: ...

# llm/pipeline.py
def summarize(snap: Snapshot, llm_cfg: LLMConfig, *, db: StateDB | None = None) -> SummaryResult | None: ...
```

---

## Task 0: 环境 + 依赖

**Files:**
- Modify: `C:\Users\<your-username>\sjtu-daily\pyproject.toml`

- [ ] **Step 1: 确认 v1 状态全绿**

```powershell
cd C:\Users\<your-username>\sjtu-daily
.\.venv\Scripts\Activate.ps1
pytest -v
```

Expected: v1 全部 ~50+ tests pass。

- [ ] **Step 2: 改 `pyproject.toml` 加依赖**

文件 `C:\Users\<your-username>\sjtu-daily\pyproject.toml`，把 `dependencies` 段改为：

```toml
[project]
name = "sjtu-daily"
version = "0.2.0"
description = "本地每日待办 dashboard，调用 SJTU-CLI 5 个子系统聚合 + LLM 摘要层"
requires-python = ">=3.11"
license = { text = "MIT" }
authors = [{ name = "wuyutanhongyuxin", email = "wuyutanhongyuxin@gmail.com" }]
dependencies = [
    "pyyaml>=6.0",
    "jinja2>=3.1",
    "windows-toasts>=1.1.0; sys_platform == 'win32'",
    "google-genai>=1.0.0",
    "openai>=1.50.0",
    "pydantic>=2.0",
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

- [ ] **Step 3: 安装新依赖**

```powershell
cd C:\Users\<your-username>\sjtu-daily
.\.venv\Scripts\Activate.ps1
pip install -e .[dev]
```

Expected: 安装 google-genai / openai / pydantic，无冲突。

- [ ] **Step 4: import 冒烟**

```powershell
python -c "from google import genai; print('genai ok')"
python -c "from openai import OpenAI; print('openai ok')"
python -c "from pydantic import BaseModel; print('pydantic ok')"
```

Expected: 三行 `* ok`。

- [ ] **Step 5: 跑 v1 测试确认未坏**

```powershell
pytest -v
```

Expected: 全部 ~50+ tests 仍 pass。

- [ ] **Step 6: Commit**

```powershell
cd C:\Users\<your-username>\sjtu-daily
git add pyproject.toml
git commit -m "chore: bump 0.2.0 + add google-genai / openai / pydantic for v1.1 LLM"
```

---

## Task 1: `llm/schemas.py` — pydantic 模型

**Files:**
- Create: `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\llm\__init__.py`（占位，Task 8 才填实质内容）
- Create: `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\llm\schemas.py`
- Create: `C:\Users\<your-username>\sjtu-daily\tests\llm\__init__.py`
- Create: `C:\Users\<your-username>\sjtu-daily\tests\llm\test_schemas.py`

- [ ] **Step 1: 建空 `__init__.py`**

文件 `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\llm\__init__.py`：

```python
"""sjtu-daily v1.1 LLM 摘要层。

对外稳定 API（Task 8 才把 pipeline / summarize 接进来）：

    from sjtu_daily.llm import summarize, SummaryResult, LLMError

红线：
- prompt 永不带 PII（red line 2）
- LLM 输出当 plaintext 渲染（red line 3）
- LLM 失败不影响 dashboard 主流程（red line 4）
- API key 永不入 git 永不打 log（red line 1）
"""
```

文件 `C:\Users\<your-username>\sjtu-daily\tests\llm\__init__.py`：空文件。

- [ ] **Step 2: 写失败测试 `tests/llm/test_schemas.py`**

```python
"""schemas 测试 —— pydantic 校验 + 拒绝越权字段。"""
import pytest
from pydantic import ValidationError

from sjtu_daily.llm.schemas import (
    AnalysisResult,
    LLMConfig,
    PolishedResult,
)


# ============== LLMConfig ==============

def test_llm_config_default_is_off():
    cfg = LLMConfig()
    assert cfg.mode == "off"
    assert cfg.gemini_api_key == ""
    assert cfg.deepseek_api_key == ""
    assert cfg.timeout_seconds == 15
    assert cfg.pipeline_budget_seconds == 45
    assert cfg.fallback_on_error is True
    assert cfg.max_tokens_out == 800


def test_llm_config_cascade_requires_at_least_one_key():
    """mode != off 时必须至少一个 provider 有 key。"""
    with pytest.raises(ValidationError):
        LLMConfig(mode="cascade")  # 两个 key 都空


def test_llm_config_gemini_only_requires_gemini_key():
    with pytest.raises(ValidationError):
        LLMConfig(mode="gemini_only", deepseek_api_key="dk_x")


def test_llm_config_deepseek_only_requires_deepseek_key():
    with pytest.raises(ValidationError):
        LLMConfig(mode="deepseek_only", gemini_api_key="gk_x")


def test_llm_config_cascade_one_key_ok():
    """cascade 模式只要任一 key 有就算合法（缺另一个走 fallback）。"""
    cfg = LLMConfig(mode="cascade", gemini_api_key="gk_x")
    assert cfg.mode == "cascade"


def test_llm_config_rejects_unknown_mode():
    with pytest.raises(ValidationError):
        LLMConfig(mode="bogus")


def test_llm_config_timeout_must_be_positive():
    with pytest.raises(ValidationError):
        LLMConfig(mode="off", timeout_seconds=0)
    with pytest.raises(ValidationError):
        LLMConfig(mode="off", timeout_seconds=-5)


def test_llm_config_max_tokens_bounded():
    """max_tokens_out 必须 1..=4000。"""
    with pytest.raises(ValidationError):
        LLMConfig(mode="off", max_tokens_out=0)
    with pytest.raises(ValidationError):
        LLMConfig(mode="off", max_tokens_out=4001)


# ============== AnalysisResult ==============

def test_analysis_result_all_lists():
    a = AnalysisResult(
        urgent=["邮件 X 待回"],
        today_highlights=["今日重点 1", "今日重点 2"],
        suggestions=["建议 1"],
        cross_cutting=["交我办和邮件都提到了截止日期"],
    )
    assert len(a.urgent) == 1
    assert len(a.today_highlights) == 2


def test_analysis_result_empty_lists_ok():
    """空 list 合法（用户当天确实没事）。"""
    a = AnalysisResult(urgent=[], today_highlights=[], suggestions=[], cross_cutting=[])
    assert a.urgent == []


def test_analysis_result_caps_urgent_at_5():
    """urgent 超过 5 条 → ValidationError。防 LLM 漂移注水。"""
    with pytest.raises(ValidationError):
        AnalysisResult(
            urgent=["1", "2", "3", "4", "5", "6"],
            today_highlights=[], suggestions=[], cross_cutting=[],
        )


def test_analysis_result_caps_today_highlights_at_5():
    with pytest.raises(ValidationError):
        AnalysisResult(
            urgent=[], today_highlights=["1"]*6, suggestions=[], cross_cutting=[],
        )


def test_analysis_result_caps_suggestions_at_3():
    with pytest.raises(ValidationError):
        AnalysisResult(
            urgent=[], today_highlights=[], suggestions=["1"]*4, cross_cutting=[],
        )


def test_analysis_result_rejects_extra_fields():
    """LLM 漂移注入未知字段 → ValidationError（schema drift gate）。"""
    with pytest.raises(ValidationError):
        AnalysisResult.model_validate({
            "urgent": [],
            "today_highlights": [],
            "suggestions": [],
            "cross_cutting": [],
            "bogus_field": "x",
        })


def test_analysis_result_rejects_non_str_items():
    with pytest.raises(ValidationError):
        AnalysisResult.model_validate({
            "urgent": [123],
            "today_highlights": [], "suggestions": [], "cross_cutting": [],
        })


# ============== PolishedResult ==============

def test_polished_result_all_strings():
    p = PolishedResult(
        today_highlights_text="今天有 3 件事...",
        urgent_text="紧急：邮件待回",
        suggestions_text="建议：优先处理 X",
        punchline="3 邮 1 待办",
    )
    assert p.punchline == "3 邮 1 待办"


def test_polished_result_caps_punchline_length():
    """punchline > 40 字 → ValidationError（Toast 显示截断）。"""
    with pytest.raises(ValidationError):
        PolishedResult(
            today_highlights_text="x",
            urgent_text="y",
            suggestions_text="z",
            punchline="一" * 41,
        )


def test_polished_result_caps_today_highlights_length():
    """today_highlights_text > 300 字 → ValidationError。"""
    with pytest.raises(ValidationError):
        PolishedResult(
            today_highlights_text="一" * 301,
            urgent_text="y", suggestions_text="z", punchline="p",
        )


def test_polished_result_rejects_extra_fields():
    with pytest.raises(ValidationError):
        PolishedResult.model_validate({
            "today_highlights_text": "x",
            "urgent_text": "y",
            "suggestions_text": "z",
            "punchline": "p",
            "extra": "no",
        })
```

- [ ] **Step 3: 跑测试确认 FAIL**

```powershell
cd C:\Users\<your-username>\sjtu-daily
pytest tests/llm/test_schemas.py -v
```

Expected: 全部 FAIL（`ModuleNotFoundError`）。

- [ ] **Step 4: 实现 `llm/schemas.py`**

文件 `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\llm\schemas.py`：

```python
"""pydantic v2 schemas —— LLMConfig 校验 + AnalysisResult / PolishedResult schema gate。

red line 8: AnalysisResult.model_validate(...) 失败视作 LLM 失败走 fallback。
"""
from __future__ import annotations

from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, model_validator


class LLMConfig(BaseModel):
    """LLM 配置。mode='off' 时跳过整个 LLM 层（v1 行为）。"""
    model_config = ConfigDict(extra="forbid", frozen=True)

    mode: Literal["off", "cascade", "gemini_only", "deepseek_only"] = "off"
    gemini_api_key: str = ""
    gemini_model: str = "gemini-2.5-flash-lite"
    deepseek_api_key: str = ""
    deepseek_model: str = "deepseek-v4-flash"
    timeout_seconds: int = Field(default=15, gt=0, le=120)
    pipeline_budget_seconds: int = Field(default=45, gt=0, le=300)
    fallback_on_error: bool = True
    max_tokens_out: int = Field(default=800, ge=1, le=4000)

    @model_validator(mode="after")
    def _at_least_one_key_when_active(self) -> "LLMConfig":
        if self.mode == "off":
            return self
        if self.mode == "gemini_only" and not self.gemini_api_key:
            raise ValueError("mode=gemini_only 需要 gemini_api_key")
        if self.mode == "deepseek_only" and not self.deepseek_api_key:
            raise ValueError("mode=deepseek_only 需要 deepseek_api_key")
        if self.mode == "cascade" and not (self.gemini_api_key or self.deepseek_api_key):
            raise ValueError("mode=cascade 至少需要 gemini_api_key 或 deepseek_api_key")
        return self


class AnalysisResult(BaseModel):
    """Gemini analyze 阶段输出（强 JSON schema gate）。

    每个 list 都有上限防 LLM 漂移注水（red line 8）。
    """
    model_config = ConfigDict(extra="forbid", frozen=True)

    urgent: list[str] = Field(default_factory=list, max_length=5)
    today_highlights: list[str] = Field(default_factory=list, max_length=5)
    suggestions: list[str] = Field(default_factory=list, max_length=3)
    cross_cutting: list[str] = Field(default_factory=list, max_length=3)


class PolishedResult(BaseModel):
    """DeepSeek polish 阶段输出 —— 中文文案 + Toast punchline。

    长度上限防止 Toast 截断 / dashboard 渲染溢出（red line 3 配套）。
    """
    model_config = ConfigDict(extra="forbid", frozen=True)

    today_highlights_text: str = Field(default="", max_length=300)
    urgent_text: str = Field(default="", max_length=200)
    suggestions_text: str = Field(default="", max_length=200)
    punchline: str = Field(default="", max_length=40)
```

- [ ] **Step 5: 跑测试确认 PASS**

```powershell
pytest tests/llm/test_schemas.py -v
```

Expected: ~17 passed。

- [ ] **Step 6: 行数 + 全测**

```powershell
(Get-Content src/sjtu_daily/llm/schemas.py | Measure-Object -Line).Lines
pytest -v
```

Expected: schemas.py ≤ 80 行；v1 全部测试仍 pass + 新 ~17 个 pass。

- [ ] **Step 7: Commit**

```powershell
git add src/sjtu_daily/llm/__init__.py src/sjtu_daily/llm/schemas.py tests/llm/__init__.py tests/llm/test_schemas.py
git commit -m "feat(llm): schemas.py pydantic LLMConfig + AnalysisResult + PolishedResult"
```

---

## Task 2: `llm/base.py` — Protocol + 错误 + 元信息

**Files:**
- Create: `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\llm\base.py`
- Create: `C:\Users\<your-username>\sjtu-daily\tests\llm\test_base.py`

- [ ] **Step 1: 写失败测试 `tests/llm/test_base.py`**

```python
"""base 测试 —— Protocol 契约 / SummaryResult / LLMError 链。"""
from sjtu_daily.llm.base import (
    LLMError,
    LLMProvider,
    RunMeta,
    SummaryResult,
)


def test_run_meta_minimal():
    rm = RunMeta(
        provider="gemini",
        model="gemini-2.5-flash-lite",
        mode="analyze",
        latency_ms=1234,
        tokens_in=500,
        tokens_out=200,
        cost_usd=0.0003,
        ok=True,
    )
    assert rm.provider == "gemini"
    assert rm.error is None


def test_run_meta_with_error():
    rm = RunMeta(
        provider="deepseek", model="deepseek-v4-flash", mode="polish",
        latency_ms=15001, tokens_in=0, tokens_out=0, cost_usd=0.0,
        ok=False, error="timeout",
    )
    assert rm.ok is False
    assert rm.error == "timeout"


def test_summary_result_fields():
    rm = RunMeta(
        provider="gemini", model="g", mode="analyze",
        latency_ms=10, tokens_in=1, tokens_out=1, cost_usd=0.0, ok=True,
    )
    s = SummaryResult(
        today_highlights_text="今日重点...",
        urgent_text="紧急...",
        suggestions_text="建议...",
        punchline="3 件事",
        runs=[rm],
    )
    assert s.punchline == "3 件事"
    assert len(s.runs) == 1


def test_llm_error_basic():
    e = LLMError("api 5xx", provider="gemini")
    assert e.provider == "gemini"
    assert e.retryable is False
    assert e.cause is None
    assert "api 5xx" in str(e)


def test_llm_error_retryable_flag():
    e = LLMError("rate limit", provider="deepseek", retryable=True)
    assert e.retryable is True


def test_llm_error_chains_cause():
    inner = RuntimeError("network down")
    e = LLMError("provider call failed", provider="gemini", cause=inner)
    assert e.cause is inner


def test_llm_provider_is_protocol():
    """LLMProvider 是 typing.Protocol —— 可用 isinstance 但不能直接实例化。"""
    # 仅检查 Protocol 接口存在
    assert hasattr(LLMProvider, "generate_json")
    assert hasattr(LLMProvider, "generate_text")


def test_provider_duck_typing_via_simple_stub():
    """任何提供 name / generate_json / generate_text 的类都 conform。"""
    class Stub:
        name = "stub"
        def generate_json(self, prompt, *, schema, timeout):
            return {"urgent": [], "today_highlights": [], "suggestions": [], "cross_cutting": []}, RunMeta(
                provider="stub", model="m", mode="analyze",
                latency_ms=1, tokens_in=1, tokens_out=1, cost_usd=0.0, ok=True,
            )
        def generate_text(self, prompt, *, max_tokens, timeout):
            return "ok", RunMeta(
                provider="stub", model="m", mode="polish",
                latency_ms=1, tokens_in=1, tokens_out=1, cost_usd=0.0, ok=True,
            )

    stub: LLMProvider = Stub()  # type: ignore[assignment]
    assert stub.name == "stub"
    d, _ = stub.generate_json("p", schema={}, timeout=1)
    assert "urgent" in d
```

- [ ] **Step 2: 跑测试确认 FAIL**

```powershell
pytest tests/llm/test_base.py -v
```

Expected: 全部 FAIL（`ModuleNotFoundError`）。

- [ ] **Step 3: 实现 `llm/base.py`**

文件 `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\llm\base.py`：

```python
"""LLM 抽象层 —— Protocol + dataclass + 错误类型。

red line 4: LLMError 由 cli.py 顶层 catch；任何 provider 调用失败都包成 LLMError
（cause= 原异常），不让原始 SDK 异常逃出。
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Protocol


@dataclass(frozen=True)
class RunMeta:
    """单次 LLM call 元信息（写 state.db / 用户层观测）。"""
    provider: str          # "gemini" / "deepseek"
    model: str             # 具体模型 ID
    mode: str              # "analyze" / "polish" / "fallback_full"
    latency_ms: int
    tokens_in: int
    tokens_out: int
    cost_usd: float        # 估算
    ok: bool
    error: str | None = None


@dataclass(frozen=True)
class SummaryResult:
    """pipeline.summarize() 成功路径返回。

    红线：所有 *_text / punchline 字段都是 plaintext，由 Jinja2 autoescape 兜底
    （red line 3）。
    """
    today_highlights_text: str
    urgent_text: str
    suggestions_text: str
    punchline: str
    runs: list[RunMeta] = field(default_factory=list)


class LLMError(Exception):
    """LLM 层统一错误。cause 链原始 SDK 异常。retryable=True 时 pipeline 可重试一次。"""

    def __init__(
        self,
        msg: str,
        *,
        provider: str,
        retryable: bool = False,
        cause: Exception | None = None,
    ) -> None:
        super().__init__(msg)
        self.provider = provider
        self.retryable = retryable
        self.cause = cause


class LLMProvider(Protocol):
    """LLM provider 契约。GeminiProvider / DeepSeekProvider 都实现这个 Protocol。

    所有方法约束：
    - 失败必须 raise LLMError（不让 SDK 原始异常逃出）
    - 成功必须返回 (payload, RunMeta)
    - timeout 单位是秒，超时算 LLMError(retryable=True)
    """

    name: str

    def generate_json(
        self,
        prompt: str,
        *,
        schema: dict[str, Any],
        timeout: int,
    ) -> tuple[dict[str, Any], RunMeta]:
        ...

    def generate_text(
        self,
        prompt: str,
        *,
        max_tokens: int,
        timeout: int,
    ) -> tuple[str, RunMeta]:
        ...
```

- [ ] **Step 4: 跑测试确认 PASS**

```powershell
pytest tests/llm/test_base.py -v
```

Expected: 8 passed。

- [ ] **Step 5: 行数检查**

```powershell
(Get-Content src/sjtu_daily/llm/base.py | Measure-Object -Line).Lines
```

Expected: ≤ 90 行。

- [ ] **Step 6: Commit**

```powershell
git add src/sjtu_daily/llm/base.py tests/llm/test_base.py
git commit -m "feat(llm): base.py LLMProvider Protocol + SummaryResult + LLMError + RunMeta"
```

---

## Task 3: `llm/prompts.py` — 3 段 prompt + 白名单字段渲染

**Files:**
- Create: `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\llm\prompts.py`
- Create: `C:\Users\<your-username>\sjtu-daily\tests\llm\test_prompts.py`

- [ ] **Step 1: 写失败测试 `tests/llm/test_prompts.py`**

```python
"""prompts 测试 —— 0 PII 守门（red line 2）+ 5 section 渲染。"""
from decimal import Decimal

from sjtu_daily.llm.prompts import (
    ANALYZE_PROMPT,
    FALLBACK_FULL_PROMPT,
    POLISH_PROMPT,
    build_analyze_input,
    build_fallback_input,
    build_polish_input,
)
from sjtu_daily.llm.schemas import AnalysisResult
from sjtu_daily.runner import CategoryResult, Snapshot


_PII_STRINGS = [
    "secret@example.com",
    "from_address",
    "from_display",
    "正文片段绝密",
    "fragment_text_xxx",
    "excerpt_text_xxx",
    "body_plain",
    "学号123456",
    "owner_real_name",
]


def _polluted_snapshot() -> Snapshot:
    """故意把 PII 塞进 items 测白名单 drop。"""
    return Snapshot(results={
        "mail": CategoryResult(
            category="mail", ok=True,
            items=[{
                "id": "M1",
                "subject": "测试邮件主题",
                "date_ms": 1716268800000,
                "unread": True,
                # PII 不应该出现在 prompt
                "from_address": "secret@example.com",
                "from_display": "from_display_should_drop",
                "fragment": "fragment_text_xxx",
            }],
        ),
        "messages": CategoryResult(
            category="messages", ok=True,
            items=[{"id": "G1", "title": "教学秘书通知", "unread_num": 2, "create_time": "2026-05-21 08:00:00"}],
        ),
        "services": CategoryResult(
            category="services", ok=True,
            items=[{
                "id": "S1", "title": "学位申请", "bucket": "my_applications",
                "step_name": "填写申请", "app_name": "学位评定", "assign_time": 1716268800,
            }],
        ),
        "shuiyuan": CategoryResult(
            category="shuiyuan", ok=True,
            items=[{
                "id": "T1", "title": "水源测试帖",
                "last_posted_at": "2026-05-21T08:00:00Z", "reply_count": 4,
                # PII 不应该出现
                "excerpt": "excerpt_text_xxx",
            }],
        ),
        "card": CategoryResult(
            category="card", ok=True, items=[],
            card_balance={"card_no_redacted": "0012***", "balance": Decimal("12.34"), "lost": False, "frozen": False},
        ),
    })


# ============== build_analyze_input ==============

def test_analyze_input_contains_5_sections():
    snap = _polluted_snapshot()
    text = build_analyze_input(snap)
    assert "邮箱" in text or "mail" in text
    assert "消息" in text or "messages" in text
    assert "办事" in text or "services" in text
    assert "水源" in text or "shuiyuan" in text
    assert "一卡通" in text or "card" in text


def test_analyze_input_includes_whitelisted_fields():
    snap = _polluted_snapshot()
    text = build_analyze_input(snap)
    assert "测试邮件主题" in text
    assert "教学秘书通知" in text
    assert "学位申请" in text
    assert "水源测试帖" in text
    assert "12.34" in text


def test_analyze_input_zero_pii():
    """red line 2 守门：prompt 文本不能含任何 PII。"""
    snap = _polluted_snapshot()
    text = build_analyze_input(snap)
    for pii in _PII_STRINGS:
        assert pii not in text, f"PII 泄漏: {pii!r} 出现在 prompt 里"


def test_analyze_input_handles_auth_required():
    """有 category auth_required 时 prompt 显式提示。"""
    snap = Snapshot(results={
        "mail": CategoryResult(category="mail", ok=False, error="SessionExpired", auth_required=True),
        "messages": CategoryResult(category="messages", ok=True, items=[]),
        "services": CategoryResult(category="services", ok=True, items=[]),
        "shuiyuan": CategoryResult(category="shuiyuan", ok=True, items=[]),
        "card": CategoryResult(category="card", ok=True, items=[], card_balance=None),
    })
    text = build_analyze_input(snap)
    assert "session" in text.lower() or "过期" in text or "auth" in text.lower()


def test_analyze_input_empty_snapshot():
    """全空 snapshot：prompt 仍合法（LLM 应输出空 list）。"""
    snap = Snapshot(results={
        cat: CategoryResult(category=cat, ok=True, items=[])
        for cat in ["mail", "messages", "services", "shuiyuan", "card"]
    })
    text = build_analyze_input(snap)
    assert len(text) > 0


# ============== build_polish_input ==============

def test_polish_input_uses_analysis():
    a = AnalysisResult(
        urgent=["邮件 X 待回"],
        today_highlights=["今日重点 A", "今日重点 B"],
        suggestions=["建议 Z"],
        cross_cutting=["跨子系统观察"],
    )
    text = build_polish_input(a)
    assert "邮件 X 待回" in text
    assert "今日重点 A" in text
    assert "建议 Z" in text


def test_polish_input_handles_empty_analysis():
    a = AnalysisResult(urgent=[], today_highlights=[], suggestions=[], cross_cutting=[])
    text = build_polish_input(a)
    assert len(text) > 0  # 仍然合法 prompt


# ============== build_fallback_input ==============

def test_fallback_input_combines_everything():
    """fallback_full prompt = analyze + polish 合并，让单 LLM 一步出 PolishedResult。"""
    snap = _polluted_snapshot()
    text = build_fallback_input(snap)
    # 必须含原始数据
    assert "测试邮件主题" in text
    # 必须含 schema 提示
    assert "punchline" in text
    assert "today_highlights_text" in text


def test_fallback_input_zero_pii():
    snap = _polluted_snapshot()
    text = build_fallback_input(snap)
    for pii in _PII_STRINGS:
        assert pii not in text


# ============== prompts 常量本身 ==============

def test_analyze_prompt_demands_plaintext():
    """red line 3: prompt 显式告诉 LLM 不要输出 HTML / markdown link。"""
    assert "plain text" in ANALYZE_PROMPT.lower() or "纯文本" in ANALYZE_PROMPT
    # 注意：ANALYZE_PROMPT 输出 JSON，"无 HTML" 主要约束 string 字段的值内容
    assert "html" in ANALYZE_PROMPT.lower() or "HTML" in ANALYZE_PROMPT


def test_polish_prompt_demands_plaintext():
    assert "plain text" in POLISH_PROMPT.lower() or "纯文本" in POLISH_PROMPT
    assert "html" in POLISH_PROMPT.lower() or "HTML" in POLISH_PROMPT


def test_fallback_prompt_demands_plaintext():
    assert "plain text" in FALLBACK_FULL_PROMPT.lower() or "纯文本" in FALLBACK_FULL_PROMPT
```

- [ ] **Step 2: 跑测试确认 FAIL**

```powershell
pytest tests/llm/test_prompts.py -v
```

Expected: 全部 FAIL（`ModuleNotFoundError`）。

- [ ] **Step 3: 实现 `llm/prompts.py`**

文件 `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\llm\prompts.py`：

```python
"""LLM prompt 模板 + 白名单字段渲染 —— red line 2 PII 守门核心。

字段白名单（**只有这些字段**能进 prompt）：
- mail: subject, date_ms, unread
- messages: title, unread_num, create_time
- services: title, bucket, step_name, app_name, assign_time
- shuiyuan: title, last_posted_at, reply_count
- card: balance, lost, frozen, card_no_redacted

任何不在此清单内的字段（from_address / fragment / excerpt / body_*）
**绝不渲染**。test_prompts.test_analyze_input_zero_pii 守门。
"""
from __future__ import annotations

from decimal import Decimal

from sjtu_daily.llm.schemas import AnalysisResult
from sjtu_daily.runner import Snapshot

# ============================================================
# Prompt 常量 —— 显式约束 plain text / no HTML（red line 3）
# ============================================================

ANALYZE_PROMPT = """你是 SJTU 校园数据分析助手。下面是某用户今天的 5 个子系统状态快照。

请按 JSON schema 返回，分析出：
1. urgent（最多 5 条）：今天必须立刻处理的事（紧急邮件 / 即将逾期待办）
2. today_highlights（最多 5 条）：今天的重点（不一定紧急）
3. suggestions（最多 3 条）：基于跨子系统数据的建议
4. cross_cutting（最多 3 条）：跨子系统观察（如多个系统都提到同一截止日）

每个字符串必须是**纯文本 plain text**，**不要 HTML 标签**，**不要 markdown 链接语法**，
**不要包含个人信息**（姓名 / 学号 / 邮箱地址 / 完整账号）。

只输出符合 schema 的 JSON，不要其它内容。

==== 用户今日数据 ====

{data}
"""

POLISH_PROMPT = """下面是已经分析过的 4 段要点 JSON。请改写成 4 段更自然的中文文案：

要求：
1. today_highlights_text（≤300 字）：把 today_highlights 列表润色成 1 段自然语言
2. urgent_text（≤200 字）：把 urgent 列表润色成 1 段，语气紧迫但不焦虑
3. suggestions_text（≤200 字）：把 suggestions + cross_cutting 合并润色成 1 段
4. punchline（≤40 字）：一句话总结今天，用于 Windows Toast 通知 body

红线：
- 每段必须是**纯文本 plain text**
- **不要 HTML 标签** / **不要 markdown 链接**
- **不要个人信息**
- 只输出符合 schema 的 JSON，不要其它内容

==== 已分析的要点（JSON）====

{analysis}
"""

FALLBACK_FULL_PROMPT = """你是 SJTU 校园数据分析助手。下面是某用户今天的 5 个子系统状态快照。

请一步到位输出符合下面 schema 的 JSON：

  {{
    "today_highlights_text": "今日重点段（≤300 字 中文纯文本）",
    "urgent_text": "紧急段（≤200 字 中文纯文本）",
    "suggestions_text": "建议段（≤200 字 中文纯文本）",
    "punchline": "一句话（≤40 字 中文纯文本，用于 Toast 通知 body）"
  }}

红线：
- 每段必须是**纯文本 plain text**
- **不要 HTML 标签** / **不要 markdown 链接**
- **不要个人信息**
- 只输出 JSON，不要其它内容

==== 用户今日数据 ====

{data}
"""


# ============================================================
# 白名单字段渲染
# ============================================================


def _fmt_date_ms(ms: int | None) -> str:
    if not ms:
        return ""
    # 简单格式化，不引入时区库（LLM 自己理解）
    return f"ts={ms}"


def _render_mail(items: list[dict]) -> str:
    if not items:
        return "（无未读邮件）"
    lines = []
    for it in items:
        subject = str(it.get("subject") or "(无主题)")
        date_hint = _fmt_date_ms(it.get("date_ms"))
        unread = "未读" if it.get("unread") else "已读"
        lines.append(f"- [{unread}] {subject} ({date_hint})")
    return "\n".join(lines)


def _render_messages(items: list[dict]) -> str:
    if not items:
        return "（无未读消息分组）"
    lines = []
    for g in items:
        title = str(g.get("title") or "(无标题)")
        n = int(g.get("unread_num", 0))
        t = str(g.get("create_time") or "")
        lines.append(f"- {title}（未读 {n} 条，{t}）")
    return "\n".join(lines)


def _render_services(items: list[dict]) -> str:
    if not items:
        return "（无待办）"
    lines = []
    for s in items:
        title = str(s.get("title") or "(无标题)")
        bucket = s.get("bucket", "")
        step = str(s.get("step_name") or "")
        app = str(s.get("app_name") or "")
        bucket_label = "我申请的" if bucket == "my_applications" else "等我处理"
        lines.append(f"- [{bucket_label}] {title}（{app} · 当前步骤: {step}）")
    return "\n".join(lines)


def _render_shuiyuan(items: list[dict]) -> str:
    if not items:
        return "（无新帖）"
    lines = []
    for t in items:
        title = str(t.get("title") or "(无标题)")
        replies = int(t.get("reply_count", 0))
        last = str(t.get("last_posted_at") or "")
        lines.append(f"- {title}（{replies} 回复 · 最后回复 {last}）")
    return "\n".join(lines)


def _render_card(balance: dict | None) -> str:
    if not balance:
        return "（无一卡通数据）"
    bal = balance.get("balance", Decimal("0"))
    if not isinstance(bal, (Decimal, str, int, float)):
        bal = "0"
    bal_str = f"{Decimal(str(bal)):.2f}"
    redacted = str(balance.get("card_no_redacted", ""))
    flags = []
    if balance.get("lost"):
        flags.append("已挂失")
    if balance.get("frozen"):
        flags.append("已冻结")
    flag_text = (" · " + " · ".join(flags)) if flags else ""
    return f"卡号 {redacted} · 余额 ¥{bal_str}{flag_text}"


def _render_category_block(name: str, snap: Snapshot, key: str, renderer) -> str:
    res = snap.results.get(key)
    if res is None:
        return f"## {name}\n（数据缺失）"
    if res.auth_required:
        return f"## {name}\n⚠️ session 过期，本次未取到数据（auth_required）"
    if not res.ok:
        return f"## {name}\n⚠️ 查询失败：{res.error or '未知错误'}"
    if key == "card":
        body = renderer(res.card_balance)
    else:
        body = renderer(res.items)
    return f"## {name}\n{body}"


def build_analyze_input(snap: Snapshot) -> str:
    """渲染 5 个子系统的白名单字段为 markdown-ish 文本（喂 Gemini analyze）。

    red line 2: 只渲染白名单字段；任何额外字段（from_address / fragment / excerpt）
    都不会出现，因为本函数根本不读它们。
    """
    blocks = [
        _render_category_block("📬 邮箱未读", snap, "mail", _render_mail),
        _render_category_block("📨 交我办消息", snap, "messages", _render_messages),
        _render_category_block("📋 办事大厅待办", snap, "services", _render_services),
        _render_category_block("💧 水源最新", snap, "shuiyuan", _render_shuiyuan),
        _render_category_block("💳 一卡通", snap, "card", _render_card),
    ]
    data = "\n\n".join(blocks)
    return ANALYZE_PROMPT.format(data=data)


def build_polish_input(analysis: AnalysisResult) -> str:
    """把 AnalysisResult 序列化进 POLISH_PROMPT。"""
    # 用 pydantic v2 model_dump_json 保证稳定 JSON（不写自定义序列化）
    return POLISH_PROMPT.format(analysis=analysis.model_dump_json(indent=2))


def build_fallback_input(snap: Snapshot) -> str:
    """单 LLM 模式：合并 analyze + polish 走一步。"""
    blocks = [
        _render_category_block("📬 邮箱未读", snap, "mail", _render_mail),
        _render_category_block("📨 交我办消息", snap, "messages", _render_messages),
        _render_category_block("📋 办事大厅待办", snap, "services", _render_services),
        _render_category_block("💧 水源最新", snap, "shuiyuan", _render_shuiyuan),
        _render_category_block("💳 一卡通", snap, "card", _render_card),
    ]
    return FALLBACK_FULL_PROMPT.format(data="\n\n".join(blocks))
```

- [ ] **Step 4: 跑测试确认 PASS**

```powershell
pytest tests/llm/test_prompts.py -v
```

Expected: 11 passed。

- [ ] **Step 5: 行数检查**

```powershell
(Get-Content src/sjtu_daily/llm/prompts.py | Measure-Object -Line).Lines
```

Expected: ≤ 200 行。如果超了拆 `prompts.py` + `prompt_renderers.py`。

- [ ] **Step 6: Commit**

```powershell
git add src/sjtu_daily/llm/prompts.py tests/llm/test_prompts.py
git commit -m "feat(llm): prompts.py 3 段 prompt + 白名单字段渲染（red line 2 守门）"
```

---

## Task 4: `llm/gemini.py` — GeminiProvider

**Files:**
- Create: `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\llm\gemini.py`
- Create: `C:\Users\<your-username>\sjtu-daily\tests\llm\test_gemini.py`

- [ ] **Step 1: 写失败测试 `tests/llm/test_gemini.py`**

```python
"""GeminiProvider 测试 —— mock google-genai SDK。"""
import json
from unittest.mock import MagicMock

import pytest

from sjtu_daily.llm.base import LLMError
from sjtu_daily.llm.gemini import GeminiProvider


def _fake_client_factory(monkeypatch, response_text: str, usage_in=100, usage_out=50):
    """返回一个工厂：调用 GeminiProvider(api_key=...).client 时返我们的 fake client。"""
    fake_response = MagicMock()
    fake_response.text = response_text
    fake_response.usage_metadata = MagicMock(
        prompt_token_count=usage_in,
        candidates_token_count=usage_out,
    )

    fake_models = MagicMock()
    fake_models.generate_content.return_value = fake_response

    fake_client = MagicMock()
    fake_client.models = fake_models

    monkeypatch.setattr(
        "sjtu_daily.llm.gemini._make_client",
        lambda api_key: fake_client,
    )
    return fake_client


def test_generate_json_returns_parsed_dict_and_meta(monkeypatch):
    payload = {
        "urgent": ["x"],
        "today_highlights": [],
        "suggestions": [],
        "cross_cutting": [],
    }
    _fake_client_factory(monkeypatch, json.dumps(payload))
    p = GeminiProvider(api_key="gk_x", model="gemini-2.5-flash-lite")
    data, meta = p.generate_json("prompt", schema={"type": "object"}, timeout=15)
    assert data == payload
    assert meta.provider == "gemini"
    assert meta.mode == "analyze"
    assert meta.ok is True
    assert meta.tokens_in == 100
    assert meta.tokens_out == 50
    assert meta.cost_usd >= 0  # 至少非负


def test_generate_json_raises_on_invalid_json(monkeypatch):
    _fake_client_factory(monkeypatch, "not json at all")
    p = GeminiProvider(api_key="gk_x", model="gemini-2.5-flash-lite")
    with pytest.raises(LLMError) as exc:
        p.generate_json("prompt", schema={"type": "object"}, timeout=15)
    assert exc.value.provider == "gemini"
    assert "json" in str(exc.value).lower() or "parse" in str(exc.value).lower()


def test_generate_json_raises_on_sdk_exception(monkeypatch):
    """SDK 抛 → 包成 LLMError(cause=...)，不让原始异常逃出。"""
    fake_client = MagicMock()
    fake_client.models.generate_content.side_effect = RuntimeError("API 500")
    monkeypatch.setattr("sjtu_daily.llm.gemini._make_client", lambda api_key: fake_client)

    p = GeminiProvider(api_key="gk_x", model="gemini-2.5-flash-lite")
    with pytest.raises(LLMError) as exc:
        p.generate_json("prompt", schema={"type": "object"}, timeout=15)
    assert exc.value.provider == "gemini"
    assert isinstance(exc.value.cause, RuntimeError)


def test_generate_text_returns_string_and_meta(monkeypatch):
    _fake_client_factory(monkeypatch, "纯文本输出")
    p = GeminiProvider(api_key="gk_x", model="gemini-2.5-flash-lite")
    text, meta = p.generate_text("prompt", max_tokens=500, timeout=15)
    assert text == "纯文本输出"
    assert meta.provider == "gemini"
    assert meta.mode == "polish"
    assert meta.ok is True


def test_generate_text_raises_on_sdk_exception(monkeypatch):
    fake_client = MagicMock()
    fake_client.models.generate_content.side_effect = RuntimeError("API 429")
    monkeypatch.setattr("sjtu_daily.llm.gemini._make_client", lambda api_key: fake_client)

    p = GeminiProvider(api_key="gk_x", model="gemini-2.5-flash-lite")
    with pytest.raises(LLMError) as exc:
        p.generate_text("prompt", max_tokens=500, timeout=15)
    assert exc.value.provider == "gemini"


def test_provider_name_attribute():
    p = GeminiProvider(api_key="gk_x", model="gemini-2.5-flash-lite")
    assert p.name == "gemini"


def test_cost_estimate_uses_token_counts(monkeypatch):
    """cost_usd 应该和 token count 成正比（基本健康度）。"""
    _fake_client_factory(monkeypatch, json.dumps({"urgent": [], "today_highlights": [], "suggestions": [], "cross_cutting": []}), usage_in=1000, usage_out=1000)
    p = GeminiProvider(api_key="gk_x", model="gemini-2.5-flash-lite")
    _, meta = p.generate_json("p", schema={"type": "object"}, timeout=15)
    cost_1k = meta.cost_usd

    _fake_client_factory(monkeypatch, json.dumps({"urgent": [], "today_highlights": [], "suggestions": [], "cross_cutting": []}), usage_in=2000, usage_out=2000)
    _, meta2 = p.generate_json("p", schema={"type": "object"}, timeout=15)
    # token 翻倍 cost 应翻倍
    assert meta2.cost_usd > cost_1k


def test_api_key_never_in_repr():
    """red line 1: api_key 不能出现在 repr/str。"""
    p = GeminiProvider(api_key="gk_my_secret_key_aaaa", model="gemini-2.5-flash-lite")
    r = repr(p)
    assert "gk_my_secret_key_aaaa" not in r
```

- [ ] **Step 2: 跑测试确认 FAIL**

```powershell
pytest tests/llm/test_gemini.py -v
```

Expected: 全部 FAIL（`ModuleNotFoundError`）。

- [ ] **Step 3: 实现 `llm/gemini.py`**

文件 `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\llm\gemini.py`：

```python
"""GeminiProvider —— google-genai SDK 实现。

red line 1: api_key 不入 __repr__ / __str__ / log。
red line 4: 所有 SDK 异常包成 LLMError(cause=...)。
"""
from __future__ import annotations

import json
import logging
import time
from typing import Any

from sjtu_daily.llm.base import LLMError, RunMeta

log = logging.getLogger(__name__)

# 价格表（单位: USD per 1M tokens，2026-05 现价；过时更新这里即可）
# Gemini 2.5 Flash-Lite: $0.10 input / $0.40 output per 1M tokens
_PRICE_TABLE: dict[str, tuple[float, float]] = {
    "gemini-2.5-flash-lite": (0.10 / 1_000_000, 0.40 / 1_000_000),
    "gemini-2.5-flash": (0.30 / 1_000_000, 2.50 / 1_000_000),
}
_DEFAULT_PRICE = (0.10 / 1_000_000, 0.40 / 1_000_000)


def _make_client(api_key: str):
    """延迟 import google-genai —— 防全局 import 时拉慢启动。"""
    from google import genai  # type: ignore[import]
    return genai.Client(api_key=api_key)


def _estimate_cost(model: str, tokens_in: int, tokens_out: int) -> float:
    p_in, p_out = _PRICE_TABLE.get(model, _DEFAULT_PRICE)
    return tokens_in * p_in + tokens_out * p_out


def _key_fingerprint(api_key: str) -> str:
    """红线 1: 日志 / repr 只露前 8 位 + ***。"""
    if not api_key:
        return "(empty)"
    return api_key[:8] + "***"


class GeminiProvider:
    """LLMProvider 实现 —— Gemini 2.5 Flash-Lite 走 google-genai SDK。"""

    name = "gemini"

    def __init__(self, *, api_key: str, model: str) -> None:
        if not api_key:
            raise LLMError("gemini api_key 为空", provider="gemini")
        self._api_key = api_key
        self._model = model
        # client 延迟初始化（mock 友好）
        self._client = None

    def __repr__(self) -> str:
        # red line 1: 永不打完整 key
        return f"GeminiProvider(model={self._model!r}, key={_key_fingerprint(self._api_key)})"

    def _ensure_client(self):
        if self._client is None:
            self._client = _make_client(self._api_key)
        return self._client

    def generate_json(
        self,
        prompt: str,
        *,
        schema: dict[str, Any],
        timeout: int,
    ) -> tuple[dict[str, Any], RunMeta]:
        """JSON mode —— response_schema 强制结构化输出。"""
        client = self._ensure_client()
        start = time.monotonic()
        try:
            response = client.models.generate_content(
                model=self._model,
                contents=prompt,
                config={
                    "response_mime_type": "application/json",
                    "response_schema": schema,
                    "max_output_tokens": 2000,
                },
            )
        except Exception as e:  # SDK 内部异常类型多变
            latency = int((time.monotonic() - start) * 1000)
            log.warning("gemini generate_content failed key=%s err=%s", _key_fingerprint(self._api_key), e)
            raise LLMError(
                f"gemini API call failed: {e}",
                provider="gemini",
                retryable=True,
                cause=e,
            )
        latency = int((time.monotonic() - start) * 1000)

        text = getattr(response, "text", None) or ""
        tokens_in, tokens_out = _extract_usage(response)
        meta = RunMeta(
            provider="gemini",
            model=self._model,
            mode="analyze",
            latency_ms=latency,
            tokens_in=tokens_in,
            tokens_out=tokens_out,
            cost_usd=_estimate_cost(self._model, tokens_in, tokens_out),
            ok=True,
        )
        try:
            data = json.loads(text)
        except json.JSONDecodeError as e:
            raise LLMError(
                f"gemini 返回非 JSON: {e}",
                provider="gemini",
                retryable=False,
                cause=e,
            )
        if not isinstance(data, dict):
            raise LLMError(f"gemini JSON 不是 object: {type(data)}", provider="gemini")
        return data, meta

    def generate_text(
        self,
        prompt: str,
        *,
        max_tokens: int,
        timeout: int,
    ) -> tuple[str, RunMeta]:
        client = self._ensure_client()
        start = time.monotonic()
        try:
            response = client.models.generate_content(
                model=self._model,
                contents=prompt,
                config={"max_output_tokens": max_tokens},
            )
        except Exception as e:
            log.warning("gemini generate_text failed key=%s err=%s", _key_fingerprint(self._api_key), e)
            raise LLMError(
                f"gemini API call failed: {e}",
                provider="gemini", retryable=True, cause=e,
            )
        latency = int((time.monotonic() - start) * 1000)

        text = getattr(response, "text", None) or ""
        tokens_in, tokens_out = _extract_usage(response)
        meta = RunMeta(
            provider="gemini",
            model=self._model,
            mode="polish",
            latency_ms=latency,
            tokens_in=tokens_in,
            tokens_out=tokens_out,
            cost_usd=_estimate_cost(self._model, tokens_in, tokens_out),
            ok=True,
        )
        return text, meta


def _extract_usage(response) -> tuple[int, int]:
    """从 response.usage_metadata 抓 token 数（None 安全）。"""
    usage = getattr(response, "usage_metadata", None)
    if usage is None:
        return 0, 0
    return (
        int(getattr(usage, "prompt_token_count", 0) or 0),
        int(getattr(usage, "candidates_token_count", 0) or 0),
    )
```

- [ ] **Step 4: 跑测试确认 PASS**

```powershell
pytest tests/llm/test_gemini.py -v
```

Expected: 8 passed。

- [ ] **Step 5: 行数检查**

```powershell
(Get-Content src/sjtu_daily/llm/gemini.py | Measure-Object -Line).Lines
```

Expected: ≤ 180 行。

- [ ] **Step 6: Commit**

```powershell
git add src/sjtu_daily/llm/gemini.py tests/llm/test_gemini.py
git commit -m "feat(llm): GeminiProvider google-genai SDK + response_schema + key redact"
```

---

## Task 5: `llm/deepseek.py` — DeepSeekProvider

**Files:**
- Create: `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\llm\deepseek.py`
- Create: `C:\Users\<your-username>\sjtu-daily\tests\llm\test_deepseek.py`

- [ ] **Step 1: 写失败测试 `tests/llm/test_deepseek.py`**

```python
"""DeepSeekProvider 测试 —— mock openai SDK。"""
import json
from unittest.mock import MagicMock

import pytest

from sjtu_daily.llm.base import LLMError
from sjtu_daily.llm.deepseek import DeepSeekProvider


def _fake_client(monkeypatch, response_content: str, in_tokens=80, out_tokens=40):
    """构造 fake openai.OpenAI client。"""
    fake_msg = MagicMock(content=response_content)
    fake_choice = MagicMock(message=fake_msg)
    fake_usage = MagicMock(prompt_tokens=in_tokens, completion_tokens=out_tokens)
    fake_resp = MagicMock(choices=[fake_choice], usage=fake_usage)

    fake_completions = MagicMock()
    fake_completions.create.return_value = fake_resp

    fake_chat = MagicMock(completions=fake_completions)

    fake_cli = MagicMock(chat=fake_chat)

    monkeypatch.setattr(
        "sjtu_daily.llm.deepseek._make_client",
        lambda api_key: fake_cli,
    )
    return fake_cli


def test_generate_json_returns_parsed_dict(monkeypatch):
    payload = {
        "today_highlights_text": "今日重点", "urgent_text": "紧急",
        "suggestions_text": "建议", "punchline": "p",
    }
    _fake_client(monkeypatch, json.dumps(payload))
    p = DeepSeekProvider(api_key="dk_x", model="deepseek-v4-flash")
    data, meta = p.generate_json("prompt", schema={"type": "object"}, timeout=15)
    assert data == payload
    assert meta.provider == "deepseek"
    assert meta.mode == "polish"
    assert meta.ok is True
    assert meta.tokens_in == 80
    assert meta.tokens_out == 40


def test_generate_json_uses_json_object_format(monkeypatch):
    """DeepSeek response_format={"type": "json_object"} 必须被调用。"""
    fake_cli = _fake_client(monkeypatch, json.dumps({
        "today_highlights_text": "x", "urgent_text": "y",
        "suggestions_text": "z", "punchline": "p",
    }))
    p = DeepSeekProvider(api_key="dk_x", model="deepseek-v4-flash")
    p.generate_json("prompt", schema={}, timeout=15)
    create_call = fake_cli.chat.completions.create.call_args
    assert create_call.kwargs.get("response_format") == {"type": "json_object"}


def test_generate_json_raises_on_invalid_json(monkeypatch):
    _fake_client(monkeypatch, "not json")
    p = DeepSeekProvider(api_key="dk_x", model="deepseek-v4-flash")
    with pytest.raises(LLMError) as exc:
        p.generate_json("prompt", schema={}, timeout=15)
    assert exc.value.provider == "deepseek"


def test_generate_json_raises_on_sdk_exception(monkeypatch):
    fake_cli = MagicMock()
    fake_cli.chat.completions.create.side_effect = RuntimeError("DS API 500")
    monkeypatch.setattr("sjtu_daily.llm.deepseek._make_client", lambda api_key: fake_cli)

    p = DeepSeekProvider(api_key="dk_x", model="deepseek-v4-flash")
    with pytest.raises(LLMError) as exc:
        p.generate_json("p", schema={}, timeout=15)
    assert exc.value.provider == "deepseek"
    assert isinstance(exc.value.cause, RuntimeError)


def test_generate_text_returns_string(monkeypatch):
    _fake_client(monkeypatch, "中文输出文本")
    p = DeepSeekProvider(api_key="dk_x", model="deepseek-v4-flash")
    text, meta = p.generate_text("p", max_tokens=500, timeout=15)
    assert text == "中文输出文本"
    assert meta.provider == "deepseek"
    assert meta.ok is True


def test_generate_text_passes_max_tokens(monkeypatch):
    fake_cli = _fake_client(monkeypatch, "ok")
    p = DeepSeekProvider(api_key="dk_x", model="deepseek-v4-flash")
    p.generate_text("p", max_tokens=777, timeout=15)
    create_call = fake_cli.chat.completions.create.call_args
    assert create_call.kwargs.get("max_tokens") == 777


def test_provider_name():
    p = DeepSeekProvider(api_key="dk_x", model="deepseek-v4-flash")
    assert p.name == "deepseek"


def test_api_key_never_in_repr():
    p = DeepSeekProvider(api_key="dk_my_secret_token_aaaa", model="deepseek-v4-flash")
    assert "dk_my_secret_token_aaaa" not in repr(p)


def test_base_url_is_deepseek(monkeypatch):
    """初始化时 base_url 必须指向 deepseek.com。"""
    captured = {}

    def fake_make(api_key: str):
        captured["key"] = api_key
        cli = MagicMock()
        cli.chat.completions.create.return_value = MagicMock(
            choices=[MagicMock(message=MagicMock(content=json.dumps({
                "today_highlights_text": "x", "urgent_text": "y",
                "suggestions_text": "z", "punchline": "p",
            })))],
            usage=MagicMock(prompt_tokens=1, completion_tokens=1),
        )
        return cli

    # 透过真实 _make_client 跑，看 base_url 是否 deepseek
    import sjtu_daily.llm.deepseek as ds_mod
    real_make = ds_mod._make_client

    captured_base_url = {}

    class FakeOpenAI:
        def __init__(self, *, api_key, base_url, timeout=None):
            captured_base_url["base_url"] = base_url
            captured_base_url["api_key"] = api_key
            self.chat = MagicMock()
            self.chat.completions = MagicMock()
            self.chat.completions.create.return_value = MagicMock(
                choices=[MagicMock(message=MagicMock(content=json.dumps({
                    "today_highlights_text": "x", "urgent_text": "y",
                    "suggestions_text": "z", "punchline": "p",
                })))],
                usage=MagicMock(prompt_tokens=1, completion_tokens=1),
            )

    monkeypatch.setattr("openai.OpenAI", FakeOpenAI)

    p = DeepSeekProvider(api_key="dk_real", model="deepseek-v4-flash")
    p.generate_json("p", schema={}, timeout=15)
    assert "deepseek.com" in captured_base_url["base_url"]
    assert captured_base_url["api_key"] == "dk_real"
```

- [ ] **Step 2: 跑测试确认 FAIL**

```powershell
pytest tests/llm/test_deepseek.py -v
```

Expected: 全部 FAIL（`ModuleNotFoundError`）。

- [ ] **Step 3: 实现 `llm/deepseek.py`**

文件 `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\llm\deepseek.py`：

```python
"""DeepSeekProvider —— openai SDK 兼容路径，base_url=https://api.deepseek.com。

red line 1: api_key 不入 __repr__ / log。
red line 4: 所有 SDK 异常包成 LLMError(cause=...)。
"""
from __future__ import annotations

import json
import logging
import time
from typing import Any

from sjtu_daily.llm.base import LLMError, RunMeta

log = logging.getLogger(__name__)

# DeepSeek 价格（2026-05，过时更新这里；`deepseek-chat` 别名将于 2026-07-24 弃用）
# deepseek-v4-flash: $0.14 input / $0.28 output per 1M tokens（cache miss）
# cache hit: $0.0028 / 1M（1/50 折扣），暂不计 cache 复用
_PRICE_TABLE: dict[str, tuple[float, float]] = {
    "deepseek-v4-flash": (0.14 / 1_000_000, 0.28 / 1_000_000),
    "deepseek-reasoner": (0.55 / 1_000_000, 2.19 / 1_000_000),
}
_DEFAULT_PRICE = (0.14 / 1_000_000, 0.28 / 1_000_000)


def _make_client(api_key: str):
    """延迟 import openai —— 防全局 import 拉慢启动。"""
    import openai  # type: ignore[import]
    return openai.OpenAI(
        api_key=api_key,
        base_url="https://api.deepseek.com",
    )


def _estimate_cost(model: str, tokens_in: int, tokens_out: int) -> float:
    p_in, p_out = _PRICE_TABLE.get(model, _DEFAULT_PRICE)
    return tokens_in * p_in + tokens_out * p_out


def _key_fingerprint(api_key: str) -> str:
    if not api_key:
        return "(empty)"
    return api_key[:8] + "***"


class DeepSeekProvider:
    """LLMProvider 实现 —— DeepSeek-Chat via openai SDK 兼容路径。"""

    name = "deepseek"

    def __init__(self, *, api_key: str, model: str) -> None:
        if not api_key:
            raise LLMError("deepseek api_key 为空", provider="deepseek")
        self._api_key = api_key
        self._model = model
        self._client = None

    def __repr__(self) -> str:
        return f"DeepSeekProvider(model={self._model!r}, key={_key_fingerprint(self._api_key)})"

    def _ensure_client(self):
        if self._client is None:
            self._client = _make_client(self._api_key)
        return self._client

    def generate_json(
        self,
        prompt: str,
        *,
        schema: dict[str, Any],
        timeout: int,
    ) -> tuple[dict[str, Any], RunMeta]:
        """DeepSeek schema gate 弱 —— prompt 内已贴 schema 文字 + response_format=json_object。"""
        client = self._ensure_client()
        start = time.monotonic()
        try:
            response = client.chat.completions.create(
                model=self._model,
                messages=[{"role": "user", "content": prompt}],
                response_format={"type": "json_object"},
                max_tokens=2000,
                timeout=timeout,
            )
        except Exception as e:
            log.warning(
                "deepseek call failed key=%s err=%s",
                _key_fingerprint(self._api_key), e,
            )
            raise LLMError(
                f"deepseek API call failed: {e}",
                provider="deepseek", retryable=True, cause=e,
            )
        latency = int((time.monotonic() - start) * 1000)

        content = response.choices[0].message.content or ""
        usage = response.usage
        tokens_in = int(getattr(usage, "prompt_tokens", 0) or 0)
        tokens_out = int(getattr(usage, "completion_tokens", 0) or 0)
        meta = RunMeta(
            provider="deepseek",
            model=self._model,
            mode="polish",
            latency_ms=latency,
            tokens_in=tokens_in,
            tokens_out=tokens_out,
            cost_usd=_estimate_cost(self._model, tokens_in, tokens_out),
            ok=True,
        )
        try:
            data = json.loads(content)
        except json.JSONDecodeError as e:
            raise LLMError(
                f"deepseek 返回非 JSON: {e}",
                provider="deepseek", retryable=False, cause=e,
            )
        if not isinstance(data, dict):
            raise LLMError(
                f"deepseek JSON 不是 object: {type(data)}",
                provider="deepseek",
            )
        return data, meta

    def generate_text(
        self,
        prompt: str,
        *,
        max_tokens: int,
        timeout: int,
    ) -> tuple[str, RunMeta]:
        client = self._ensure_client()
        start = time.monotonic()
        try:
            response = client.chat.completions.create(
                model=self._model,
                messages=[{"role": "user", "content": prompt}],
                max_tokens=max_tokens,
                timeout=timeout,
            )
        except Exception as e:
            log.warning(
                "deepseek text call failed key=%s err=%s",
                _key_fingerprint(self._api_key), e,
            )
            raise LLMError(
                f"deepseek API call failed: {e}",
                provider="deepseek", retryable=True, cause=e,
            )
        latency = int((time.monotonic() - start) * 1000)

        content = response.choices[0].message.content or ""
        usage = response.usage
        tokens_in = int(getattr(usage, "prompt_tokens", 0) or 0)
        tokens_out = int(getattr(usage, "completion_tokens", 0) or 0)
        meta = RunMeta(
            provider="deepseek",
            model=self._model,
            mode="polish",
            latency_ms=latency,
            tokens_in=tokens_in,
            tokens_out=tokens_out,
            cost_usd=_estimate_cost(self._model, tokens_in, tokens_out),
            ok=True,
        )
        return content, meta
```

- [ ] **Step 4: 跑测试确认 PASS**

```powershell
pytest tests/llm/test_deepseek.py -v
```

Expected: 9 passed。

- [ ] **Step 5: 行数检查**

```powershell
(Get-Content src/sjtu_daily/llm/deepseek.py | Measure-Object -Line).Lines
```

Expected: ≤ 180 行。

- [ ] **Step 6: Commit**

```powershell
git add src/sjtu_daily/llm/deepseek.py tests/llm/test_deepseek.py
git commit -m "feat(llm): DeepSeekProvider openai SDK + base_url + json_object + key redact"
```

---

## Task 6: `llm/pipeline.py` — summarize() 双向 fallback 编排

**Files:**
- Create: `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\llm\pipeline.py`
- Create: `C:\Users\<your-username>\sjtu-daily\tests\llm\test_pipeline.py`

- [ ] **Step 1: 写失败测试 `tests/llm/test_pipeline.py`**

```python
"""pipeline 测试 —— 4 路径 + cost 记账 + schema gate + budget。"""
from decimal import Decimal
from unittest.mock import MagicMock

import pytest

from sjtu_daily.llm.base import LLMError, RunMeta
from sjtu_daily.llm.pipeline import summarize
from sjtu_daily.llm.schemas import LLMConfig
from sjtu_daily.runner import CategoryResult, Snapshot


def _snap_basic() -> Snapshot:
    return Snapshot(results={
        "mail": CategoryResult(category="mail", ok=True, items=[
            {"id": "M1", "subject": "测试", "date_ms": 1716268800000, "unread": True},
        ]),
        "messages": CategoryResult(category="messages", ok=True, items=[]),
        "services": CategoryResult(category="services", ok=True, items=[]),
        "shuiyuan": CategoryResult(category="shuiyuan", ok=True, items=[]),
        "card": CategoryResult(
            category="card", ok=True, items=[],
            card_balance={"card_no_redacted": "0012***", "balance": Decimal("12.34"), "lost": False, "frozen": False},
        ),
    })


def _good_analysis_dict() -> dict:
    return {
        "urgent": ["邮件 X 待回"],
        "today_highlights": ["今日重点 A"],
        "suggestions": ["建议 Z"],
        "cross_cutting": [],
    }


def _good_polished_dict() -> dict:
    return {
        "today_highlights_text": "今天有 1 件重点",
        "urgent_text": "1 封紧急邮件",
        "suggestions_text": "建议先处理邮件",
        "punchline": "1 邮件待回",
    }


def _good_full_dict() -> dict:
    """fallback_full 直接返 PolishedResult 风格。"""
    return _good_polished_dict()


def _meta(provider="gemini", mode="analyze", ok=True):
    return RunMeta(
        provider=provider, model="m", mode=mode,
        latency_ms=100, tokens_in=10, tokens_out=10,
        cost_usd=0.0001, ok=ok,
    )


# ============== mode=off 直接返 None ==============

def test_summarize_returns_none_when_off():
    cfg = LLMConfig(mode="off")
    result = summarize(_snap_basic(), cfg)
    assert result is None


# ============== 路径 1: cascade happy path ==============

def test_summarize_cascade_happy_path(monkeypatch):
    fake_gemini = MagicMock()
    fake_gemini.name = "gemini"
    fake_gemini.generate_json.return_value = (_good_analysis_dict(), _meta("gemini", "analyze"))

    fake_ds = MagicMock()
    fake_ds.name = "deepseek"
    fake_ds.generate_json.return_value = (_good_polished_dict(), _meta("deepseek", "polish"))

    monkeypatch.setattr("sjtu_daily.llm.pipeline._make_gemini", lambda cfg: fake_gemini)
    monkeypatch.setattr("sjtu_daily.llm.pipeline._make_deepseek", lambda cfg: fake_ds)

    cfg = LLMConfig(mode="cascade", gemini_api_key="gk_x", deepseek_api_key="dk_x")
    result = summarize(_snap_basic(), cfg)
    assert result is not None
    assert result.punchline == "1 邮件待回"
    assert len(result.runs) == 2  # 1 analyze + 1 polish
    assert result.runs[0].provider == "gemini"
    assert result.runs[1].provider == "deepseek"
    # cost 累加
    total_cost = sum(r.cost_usd for r in result.runs)
    assert total_cost > 0


# ============== 路径 2: Gemini 失败 → DeepSeek 单跑 fallback ==============

def test_summarize_fallback_when_gemini_fails(monkeypatch):
    fake_gemini = MagicMock()
    fake_gemini.name = "gemini"
    fake_gemini.generate_json.side_effect = LLMError("gemini down", provider="gemini")

    fake_ds = MagicMock()
    fake_ds.name = "deepseek"
    fake_ds.generate_json.return_value = (_good_full_dict(), _meta("deepseek", "fallback_full"))

    monkeypatch.setattr("sjtu_daily.llm.pipeline._make_gemini", lambda cfg: fake_gemini)
    monkeypatch.setattr("sjtu_daily.llm.pipeline._make_deepseek", lambda cfg: fake_ds)

    cfg = LLMConfig(mode="cascade", gemini_api_key="gk_x", deepseek_api_key="dk_x")
    result = summarize(_snap_basic(), cfg)
    assert result is not None
    assert result.punchline == "1 邮件待回"
    # 至少 1 失败 + 1 fallback
    assert any(r.ok is False for r in result.runs)
    assert any(r.mode == "fallback_full" for r in result.runs)


# ============== 路径 3: DeepSeek 失败 → Gemini 输出直接当 polished 兜底 ==============

def test_summarize_uses_gemini_as_polished_when_deepseek_fails(monkeypatch):
    """Gemini analyze 成功，但 DeepSeek polish 失败：
    pipeline 退化用 AnalysisResult 字段直接拼成 SummaryResult（粗糙但能看）。"""
    fake_gemini = MagicMock()
    fake_gemini.name = "gemini"
    fake_gemini.generate_json.return_value = (_good_analysis_dict(), _meta("gemini", "analyze"))

    fake_ds = MagicMock()
    fake_ds.name = "deepseek"
    fake_ds.generate_json.side_effect = LLMError("DS down", provider="deepseek")

    monkeypatch.setattr("sjtu_daily.llm.pipeline._make_gemini", lambda cfg: fake_gemini)
    monkeypatch.setattr("sjtu_daily.llm.pipeline._make_deepseek", lambda cfg: fake_ds)

    cfg = LLMConfig(mode="cascade", gemini_api_key="gk_x", deepseek_api_key="dk_x")
    result = summarize(_snap_basic(), cfg)
    assert result is not None
    # 由 AnalysisResult 直接拼凑的兜底文本应含原始要点
    assert "邮件 X 待回" in result.urgent_text or "邮件 X 待回" in result.today_highlights_text
    # punchline 可以是空字符串或简单拼装
    assert isinstance(result.punchline, str)


# ============== 路径 4: 两个都挂 → None ==============

def test_summarize_returns_none_when_both_fail(monkeypatch):
    fake_gemini = MagicMock()
    fake_gemini.name = "gemini"
    fake_gemini.generate_json.side_effect = LLMError("g", provider="gemini")

    fake_ds = MagicMock()
    fake_ds.name = "deepseek"
    fake_ds.generate_json.side_effect = LLMError("d", provider="deepseek")

    monkeypatch.setattr("sjtu_daily.llm.pipeline._make_gemini", lambda cfg: fake_gemini)
    monkeypatch.setattr("sjtu_daily.llm.pipeline._make_deepseek", lambda cfg: fake_ds)

    cfg = LLMConfig(mode="cascade", gemini_api_key="gk_x", deepseek_api_key="dk_x")
    result = summarize(_snap_basic(), cfg)
    assert result is None


# ============== schema gate: Gemini 返回越权字段 → 视作失败 ==============

def test_summarize_treats_invalid_analysis_as_failure(monkeypatch):
    """Gemini 返回 6 条 urgent（schema 上限 5）→ pydantic 拒 → 走 fallback。"""
    bad_dict = {
        "urgent": ["1", "2", "3", "4", "5", "6"],  # 超 5 条上限
        "today_highlights": [], "suggestions": [], "cross_cutting": [],
    }
    fake_gemini = MagicMock()
    fake_gemini.name = "gemini"
    fake_gemini.generate_json.return_value = (bad_dict, _meta("gemini", "analyze"))

    fake_ds = MagicMock()
    fake_ds.name = "deepseek"
    fake_ds.generate_json.return_value = (_good_full_dict(), _meta("deepseek", "fallback_full"))

    monkeypatch.setattr("sjtu_daily.llm.pipeline._make_gemini", lambda cfg: fake_gemini)
    monkeypatch.setattr("sjtu_daily.llm.pipeline._make_deepseek", lambda cfg: fake_ds)

    cfg = LLMConfig(mode="cascade", gemini_api_key="gk_x", deepseek_api_key="dk_x")
    result = summarize(_snap_basic(), cfg)
    assert result is not None
    # 应该走了 fallback_full
    assert any(r.mode == "fallback_full" for r in result.runs)


# ============== gemini_only 模式 ==============

def test_summarize_gemini_only_skips_deepseek(monkeypatch):
    fake_gemini = MagicMock()
    fake_gemini.name = "gemini"
    fake_gemini.generate_json.return_value = (_good_full_dict(), _meta("gemini", "fallback_full"))

    fake_ds = MagicMock()
    fake_ds.name = "deepseek"

    monkeypatch.setattr("sjtu_daily.llm.pipeline._make_gemini", lambda cfg: fake_gemini)
    monkeypatch.setattr("sjtu_daily.llm.pipeline._make_deepseek", lambda cfg: fake_ds)

    cfg = LLMConfig(mode="gemini_only", gemini_api_key="gk_x")
    result = summarize(_snap_basic(), cfg)
    assert result is not None
    fake_ds.generate_json.assert_not_called()


# ============== deepseek_only 模式 ==============

def test_summarize_deepseek_only_skips_gemini(monkeypatch):
    fake_ds = MagicMock()
    fake_ds.name = "deepseek"
    fake_ds.generate_json.return_value = (_good_full_dict(), _meta("deepseek", "fallback_full"))

    fake_gemini = MagicMock()
    fake_gemini.name = "gemini"

    monkeypatch.setattr("sjtu_daily.llm.pipeline._make_gemini", lambda cfg: fake_gemini)
    monkeypatch.setattr("sjtu_daily.llm.pipeline._make_deepseek", lambda cfg: fake_ds)

    cfg = LLMConfig(mode="deepseek_only", deepseek_api_key="dk_x")
    result = summarize(_snap_basic(), cfg)
    assert result is not None
    fake_gemini.generate_json.assert_not_called()


# ============== db.record_llm_run 被调到 ==============

def test_summarize_records_every_run_to_db(monkeypatch):
    fake_gemini = MagicMock()
    fake_gemini.name = "gemini"
    fake_gemini.generate_json.return_value = (_good_analysis_dict(), _meta("gemini", "analyze"))

    fake_ds = MagicMock()
    fake_ds.name = "deepseek"
    fake_ds.generate_json.return_value = (_good_polished_dict(), _meta("deepseek", "polish"))

    monkeypatch.setattr("sjtu_daily.llm.pipeline._make_gemini", lambda cfg: fake_gemini)
    monkeypatch.setattr("sjtu_daily.llm.pipeline._make_deepseek", lambda cfg: fake_ds)

    fake_db = MagicMock()
    cfg = LLMConfig(mode="cascade", gemini_api_key="gk_x", deepseek_api_key="dk_x")
    summarize(_snap_basic(), cfg, db=fake_db)
    # 2 次成功调用 = 2 次 record_llm_run
    assert fake_db.record_llm_run.call_count == 2


def test_summarize_records_failed_runs_too(monkeypatch):
    fake_gemini = MagicMock()
    fake_gemini.name = "gemini"
    fake_gemini.generate_json.side_effect = LLMError("g down", provider="gemini")

    fake_ds = MagicMock()
    fake_ds.name = "deepseek"
    fake_ds.generate_json.side_effect = LLMError("d down", provider="deepseek")

    monkeypatch.setattr("sjtu_daily.llm.pipeline._make_gemini", lambda cfg: fake_gemini)
    monkeypatch.setattr("sjtu_daily.llm.pipeline._make_deepseek", lambda cfg: fake_ds)

    fake_db = MagicMock()
    cfg = LLMConfig(mode="cascade", gemini_api_key="gk_x", deepseek_api_key="dk_x")
    summarize(_snap_basic(), cfg, db=fake_db)
    # 失败的 run 也记一行
    assert fake_db.record_llm_run.call_count >= 1
```

- [ ] **Step 2: 跑测试确认 FAIL**

```powershell
pytest tests/llm/test_pipeline.py -v
```

Expected: 全部 FAIL（`ModuleNotFoundError`）。

- [ ] **Step 3: 实现 `llm/pipeline.py`**

文件 `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\llm\pipeline.py`：

```python
"""pipeline.summarize() —— 双向 fallback 编排。

正向路径：
  1. Gemini analyze → AnalysisResult
  2. DeepSeek polish(AnalysisResult) → PolishedResult
  3. 返回 SummaryResult

Fallback：
  - 1 失败 → 2 走 DeepSeek fallback_full（直接出 PolishedResult）
  - 2 失败 → 用 AnalysisResult 字段裸拼 SummaryResult（粗糙但能看）
  - 1+2 都失败 → 返 None（cli 走 no-summary 兜底）

模式：
  - off: 跳过整个 pipeline，返 None
  - cascade: 走 Gemini → DeepSeek 正向 + 双向 fallback
  - gemini_only: 用 GeminiProvider 单跑 fallback_full
  - deepseek_only: 用 DeepSeekProvider 单跑 fallback_full

red line 6: db.record_llm_run(meta) 每次 LLM call 都调（包括失败 run），用于成本观测。
"""
from __future__ import annotations

import logging
from typing import Any

from pydantic import ValidationError

from sjtu_daily.llm.base import LLMError, LLMProvider, RunMeta, SummaryResult
from sjtu_daily.llm.deepseek import DeepSeekProvider
from sjtu_daily.llm.gemini import GeminiProvider
from sjtu_daily.llm.prompts import (
    build_analyze_input,
    build_fallback_input,
    build_polish_input,
)
from sjtu_daily.llm.schemas import AnalysisResult, LLMConfig, PolishedResult
from sjtu_daily.runner import Snapshot

log = logging.getLogger(__name__)

# AnalysisResult JSON Schema（喂 Gemini response_schema）。
_ANALYSIS_SCHEMA: dict[str, Any] = {
    "type": "object",
    "properties": {
        "urgent": {"type": "array", "items": {"type": "string"}, "maxItems": 5},
        "today_highlights": {"type": "array", "items": {"type": "string"}, "maxItems": 5},
        "suggestions": {"type": "array", "items": {"type": "string"}, "maxItems": 3},
        "cross_cutting": {"type": "array", "items": {"type": "string"}, "maxItems": 3},
    },
    "required": ["urgent", "today_highlights", "suggestions", "cross_cutting"],
}

# PolishedResult JSON Schema（fallback_full 用，DeepSeek schema 弱靠 prompt + 校验）。
_POLISHED_SCHEMA: dict[str, Any] = {
    "type": "object",
    "properties": {
        "today_highlights_text": {"type": "string"},
        "urgent_text": {"type": "string"},
        "suggestions_text": {"type": "string"},
        "punchline": {"type": "string"},
    },
    "required": ["today_highlights_text", "urgent_text", "suggestions_text", "punchline"],
}


def _make_gemini(cfg: LLMConfig) -> GeminiProvider:
    return GeminiProvider(api_key=cfg.gemini_api_key, model=cfg.gemini_model)


def _make_deepseek(cfg: LLMConfig) -> DeepSeekProvider:
    return DeepSeekProvider(api_key=cfg.deepseek_api_key, model=cfg.deepseek_model)


def _record(db, meta: RunMeta) -> None:
    if db is None:
        return
    try:
        db.record_llm_run(meta)
    except Exception as e:
        log.warning("record_llm_run 失败（不阻塞 LLM 主路径）: %s", e)


def _meta_failed(provider: str, mode: str, err: Exception) -> RunMeta:
    return RunMeta(
        provider=provider, model="(unknown)", mode=mode,
        latency_ms=0, tokens_in=0, tokens_out=0, cost_usd=0.0,
        ok=False, error=str(err)[:200],
    )


def _try_analyze(provider: LLMProvider, snap: Snapshot, *, timeout: int, db) -> tuple[AnalysisResult, RunMeta] | None:
    """单次 Gemini analyze；schema validate 失败也算失败。返 None 表示失败。"""
    try:
        raw, meta = provider.generate_json(
            build_analyze_input(snap), schema=_ANALYSIS_SCHEMA, timeout=timeout,
        )
        _record(db, meta)
        a = AnalysisResult.model_validate(raw)  # red line 8 schema gate
        return a, meta
    except LLMError as e:
        meta = _meta_failed(provider.name, "analyze", e)
        _record(db, meta)
        log.info("analyze 失败 provider=%s: %s", provider.name, e)
        return None
    except ValidationError as e:
        # schema drift：当成失败
        meta = _meta_failed(provider.name, "analyze", e)
        _record(db, meta)
        log.info("analyze schema drift provider=%s: %s", provider.name, e)
        return None


def _try_polish(
    provider: LLMProvider, analysis: AnalysisResult, *, timeout: int, db,
) -> tuple[PolishedResult, RunMeta] | None:
    try:
        raw, meta = provider.generate_json(
            build_polish_input(analysis), schema=_POLISHED_SCHEMA, timeout=timeout,
        )
        _record(db, meta)
        p = PolishedResult.model_validate(raw)
        return p, meta
    except LLMError as e:
        meta = _meta_failed(provider.name, "polish", e)
        _record(db, meta)
        log.info("polish 失败 provider=%s: %s", provider.name, e)
        return None
    except ValidationError as e:
        meta = _meta_failed(provider.name, "polish", e)
        _record(db, meta)
        log.info("polish schema drift provider=%s: %s", provider.name, e)
        return None


def _try_fallback_full(
    provider: LLMProvider, snap: Snapshot, *, timeout: int, db,
) -> tuple[PolishedResult, RunMeta] | None:
    try:
        raw, meta = provider.generate_json(
            build_fallback_input(snap), schema=_POLISHED_SCHEMA, timeout=timeout,
        )
        # 强行覆盖 mode 标签
        meta = RunMeta(
            provider=meta.provider, model=meta.model, mode="fallback_full",
            latency_ms=meta.latency_ms, tokens_in=meta.tokens_in, tokens_out=meta.tokens_out,
            cost_usd=meta.cost_usd, ok=meta.ok, error=meta.error,
        )
        _record(db, meta)
        p = PolishedResult.model_validate(raw)
        return p, meta
    except LLMError as e:
        meta = _meta_failed(provider.name, "fallback_full", e)
        _record(db, meta)
        log.info("fallback_full 失败 provider=%s: %s", provider.name, e)
        return None
    except ValidationError as e:
        meta = _meta_failed(provider.name, "fallback_full", e)
        _record(db, meta)
        log.info("fallback_full schema drift provider=%s: %s", provider.name, e)
        return None


def _make_summary_from_analysis(analysis: AnalysisResult, runs: list[RunMeta]) -> SummaryResult:
    """DeepSeek polish 挂了的兜底：用 AnalysisResult 字段直接拼。"""
    return SummaryResult(
        today_highlights_text=" / ".join(analysis.today_highlights),
        urgent_text=" / ".join(analysis.urgent),
        suggestions_text=" / ".join(analysis.suggestions + analysis.cross_cutting),
        punchline=(analysis.urgent[0] if analysis.urgent else (analysis.today_highlights[0] if analysis.today_highlights else ""))[:40],
        runs=runs,
    )


def _make_summary_from_polished(polished: PolishedResult, runs: list[RunMeta]) -> SummaryResult:
    return SummaryResult(
        today_highlights_text=polished.today_highlights_text,
        urgent_text=polished.urgent_text,
        suggestions_text=polished.suggestions_text,
        punchline=polished.punchline,
        runs=runs,
    )


def summarize(
    snap: Snapshot,
    llm_cfg: LLMConfig,
    *,
    db=None,
) -> SummaryResult | None:
    """LLM 摘要主入口。失败时返 None；调用方（cli.py）必须接住 None 走 no-summary 兜底。

    red line 4: 本函数不应 raise；任何错误都吃成 None。
    """
    if llm_cfg.mode == "off":
        return None

    runs: list[RunMeta] = []

    # gemini_only / deepseek_only 单跑 fallback_full
    if llm_cfg.mode == "gemini_only":
        try:
            provider = _make_gemini(llm_cfg)
        except LLMError as e:
            log.warning("make gemini failed: %s", e)
            return None
        r = _try_fallback_full(provider, snap, timeout=llm_cfg.timeout_seconds, db=db)
        if r is None:
            return None
        polished, meta = r
        runs.append(meta)
        return _make_summary_from_polished(polished, runs)

    if llm_cfg.mode == "deepseek_only":
        try:
            provider = _make_deepseek(llm_cfg)
        except LLMError as e:
            log.warning("make deepseek failed: %s", e)
            return None
        r = _try_fallback_full(provider, snap, timeout=llm_cfg.timeout_seconds, db=db)
        if r is None:
            return None
        polished, meta = r
        runs.append(meta)
        return _make_summary_from_polished(polished, runs)

    # cascade 模式
    assert llm_cfg.mode == "cascade"

    # 1. Gemini analyze
    gemini = None
    if llm_cfg.gemini_api_key:
        try:
            gemini = _make_gemini(llm_cfg)
        except LLMError as e:
            log.warning("make gemini failed: %s", e)
            gemini = None

    analysis_result = None
    if gemini is not None:
        r = _try_analyze(gemini, snap, timeout=llm_cfg.timeout_seconds, db=db)
        if r is not None:
            analysis, meta = r
            analysis_result = analysis
            runs.append(meta)

    # 2. DeepSeek polish
    deepseek = None
    if llm_cfg.deepseek_api_key:
        try:
            deepseek = _make_deepseek(llm_cfg)
        except LLMError as e:
            log.warning("make deepseek failed: %s", e)
            deepseek = None

    if analysis_result is not None:
        # Gemini 成功 → 试 polish
        if deepseek is not None:
            r = _try_polish(deepseek, analysis_result, timeout=llm_cfg.timeout_seconds, db=db)
            if r is not None:
                polished, meta = r
                runs.append(meta)
                return _make_summary_from_polished(polished, runs)
        # polish 挂了 → 用 analysis 裸拼
        log.info("polish 失败或缺 deepseek key，用 analysis 裸拼兜底")
        return _make_summary_from_analysis(analysis_result, runs)

    # Gemini 失败 → 走 DeepSeek fallback_full
    if deepseek is not None and llm_cfg.fallback_on_error:
        r = _try_fallback_full(deepseek, snap, timeout=llm_cfg.timeout_seconds, db=db)
        if r is not None:
            polished, meta = r
            runs.append(meta)
            return _make_summary_from_polished(polished, runs)

    # 两个都挂
    log.warning("LLM pipeline 全挂，返回 None 走 no-summary 兜底")
    return None
```

- [ ] **Step 4: 跑测试确认 PASS**

```powershell
pytest tests/llm/test_pipeline.py -v
```

Expected: ~10 passed。

- [ ] **Step 5: 行数检查**

```powershell
(Get-Content src/sjtu_daily/llm/pipeline.py | Measure-Object -Line).Lines
```

Expected: ≤ 220 行（超 200 但 OK，如果超 250 拆 `pipeline_helpers.py`）。

- [ ] **Step 6: 全 LLM 测试**

```powershell
pytest tests/llm/ -v
```

Expected: schemas (~17) + base (8) + prompts (11) + gemini (8) + deepseek (9) + pipeline (~10) ≈ 63 passed。

- [ ] **Step 7: Commit**

```powershell
git add src/sjtu_daily/llm/pipeline.py tests/llm/test_pipeline.py
git commit -m "feat(llm): pipeline.summarize 双向 fallback + 4 路径 + schema gate + cost 记账"
```

---

## Task 7: `llm/__init__.py` re-export

**Files:**
- Modify: `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\llm\__init__.py`

- [ ] **Step 1: 改 `llm/__init__.py` 加 re-export**

文件 `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\llm\__init__.py`：

```python
"""sjtu-daily v1.1 LLM 摘要层。

对外稳定 API：

    from sjtu_daily.llm import (
        summarize,
        SummaryResult,
        LLMError,
        LLMConfig,
    )

红线：
- prompt 永不带 PII（red line 2）
- LLM 输出当 plaintext 渲染（red line 3）
- LLM 失败不影响 dashboard 主流程（red line 4）
- API key 永不入 git 永不打 log（red line 1）
"""
from sjtu_daily.llm.base import LLMError, LLMProvider, RunMeta, SummaryResult
from sjtu_daily.llm.pipeline import summarize
from sjtu_daily.llm.schemas import AnalysisResult, LLMConfig, PolishedResult

__all__ = [
    "AnalysisResult",
    "LLMConfig",
    "LLMError",
    "LLMProvider",
    "PolishedResult",
    "RunMeta",
    "SummaryResult",
    "summarize",
]
```

- [ ] **Step 2: 冒烟 import**

```powershell
cd C:\Users\<your-username>\sjtu-daily
python -c "from sjtu_daily.llm import summarize, SummaryResult, LLMError, LLMConfig; print('ok')"
```

Expected: `ok`。

- [ ] **Step 3: 全测**

```powershell
pytest -v
```

Expected: 全部仍 pass（约 v1 50+ + LLM 63 ≈ 113）。

- [ ] **Step 4: Commit**

```powershell
git add src/sjtu_daily/llm/__init__.py
git commit -m "feat(llm): __init__.py re-export 稳定 API surface"
```

---

## Task 8: 改 `config.py` 接入 LLMConfig

**Files:**
- Modify: `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\config.py`
- Modify: `C:\Users\<your-username>\sjtu-daily\tests\test_config.py`
- Modify: `C:\Users\<your-username>\sjtu-daily\.gitignore`（确认 config.toml 已 ignore）

- [ ] **Step 1: 验证 `.gitignore` 已含 `config.toml`**

```powershell
cd C:\Users\<your-username>\sjtu-daily
Select-String -Path .gitignore -Pattern "config.toml"
```

Expected: 至少 1 行匹配。如未匹配，追加：

```powershell
Add-Content -Path .gitignore -Value "`nconfig.toml`n"
```

- [ ] **Step 2: 改 `tests/test_config.py` 加 LLM 相关 case**

文件 `C:\Users\<your-username>\sjtu-daily\tests\test_config.py` 末尾追加（在已有测试后）：

```python
# ============== v1.1 LLM 配置 ==============

def test_load_config_no_llm_section_returns_off(tmp_path):
    """没有 [llm] 段：cfg.llm.mode == "off"（v1 行为不变）。"""
    cfg_file = tmp_path / "config.toml"
    cfg_file.write_text(
        '''
[sjtu_cli]
binary = "sjtu.exe"
''',
        encoding="utf-8",
    )
    c = load_config(cfg_file)
    assert c.llm.mode == "off"


def test_load_config_llm_cascade(tmp_path):
    cfg_file = tmp_path / "config.toml"
    cfg_file.write_text(
        '''
[llm]
mode = "cascade"
gemini_api_key = "gk_test"
deepseek_api_key = "dk_test"
timeout_seconds = 20
fallback_on_error = false
max_tokens_out = 1200
''',
        encoding="utf-8",
    )
    c = load_config(cfg_file)
    assert c.llm.mode == "cascade"
    assert c.llm.gemini_api_key == "gk_test"
    assert c.llm.deepseek_api_key == "dk_test"
    assert c.llm.timeout_seconds == 20
    assert c.llm.fallback_on_error is False
    assert c.llm.max_tokens_out == 1200


def test_load_config_llm_partial_fields_uses_defaults(tmp_path):
    """只填一个 key + mode → 其余字段用默认。"""
    cfg_file = tmp_path / "config.toml"
    cfg_file.write_text(
        '''
[llm]
mode = "gemini_only"
gemini_api_key = "gk_only"
''',
        encoding="utf-8",
    )
    c = load_config(cfg_file)
    assert c.llm.mode == "gemini_only"
    assert c.llm.timeout_seconds == 15  # 默认
    assert c.llm.fallback_on_error is True


def test_load_config_llm_invalid_mode_raises(tmp_path):
    """配错 mode → load_config 在 LLMConfig pydantic 校验时抛。"""
    from pydantic import ValidationError
    cfg_file = tmp_path / "config.toml"
    cfg_file.write_text(
        '''
[llm]
mode = "bogus_mode"
''',
        encoding="utf-8",
    )
    with pytest.raises(ValidationError):
        load_config(cfg_file)


def test_load_config_llm_cascade_no_keys_raises(tmp_path):
    """mode=cascade 但两个 key 都没填 → 校验抛。"""
    from pydantic import ValidationError
    cfg_file = tmp_path / "config.toml"
    cfg_file.write_text(
        '''
[llm]
mode = "cascade"
''',
        encoding="utf-8",
    )
    with pytest.raises(ValidationError):
        load_config(cfg_file)


def test_config_toml_is_gitignored():
    """red line 1: config.toml 必须在 .gitignore 里（防止 API key 入 git）。"""
    from pathlib import Path
    repo_root = Path(__file__).parent.parent
    gi = (repo_root / ".gitignore").read_text(encoding="utf-8")
    assert "config.toml" in gi
```

如果 test_config.py 原文件顶部还没 import pytest 也要加。

- [ ] **Step 3: 跑测试确认新 case FAIL**

```powershell
pytest tests/test_config.py -v
```

Expected: 旧 2 个 pass，新 6 个 FAIL（`AttributeError: 'Config' object has no attribute 'llm'`）。

- [ ] **Step 4: 改 `config.py`**

文件 `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\config.py`：

```python
"""config.toml 读取。tomllib 是 Python 3.11+ 标准库。

v1.1: 新增 [llm] 段映射到 LLMConfig（pydantic）。没有 [llm] 段时 mode="off"
（v1 行为不变）。
"""
from __future__ import annotations

import tomllib
from dataclasses import dataclass, field
from pathlib import Path

from sjtu_daily.llm.schemas import LLMConfig


@dataclass(frozen=True)
class Config:
    sjtu_binary: str
    timeout_seconds: int
    mail_limit: int
    shuiyuan_limit: int
    notify_app_name: str
    min_interval_hours: int
    llm: LLMConfig = field(default_factory=LLMConfig)


_DEFAULTS_NON_LLM = {
    "sjtu_binary": "",
    "timeout_seconds": 30,
    "mail_limit": 50,
    "shuiyuan_limit": 30,
    "notify_app_name": "SJTU Daily",
    "min_interval_hours": 6,
}


def _build_defaults() -> Config:
    return Config(
        sjtu_binary=_DEFAULTS_NON_LLM["sjtu_binary"],
        timeout_seconds=_DEFAULTS_NON_LLM["timeout_seconds"],
        mail_limit=_DEFAULTS_NON_LLM["mail_limit"],
        shuiyuan_limit=_DEFAULTS_NON_LLM["shuiyuan_limit"],
        notify_app_name=_DEFAULTS_NON_LLM["notify_app_name"],
        min_interval_hours=_DEFAULTS_NON_LLM["min_interval_hours"],
        llm=LLMConfig(),
    )


def load_config(path: Path) -> Config:
    """从 path 读 toml；不存在则返默认值。

    [llm] 段经 pydantic LLMConfig 校验；失败抛 ValidationError（上层 cli 不 catch
    —— 配置错误是显式失败，不静默）。
    """
    if not path.is_file():
        return _build_defaults()
    with path.open("rb") as f:
        raw = tomllib.load(f)

    sjtu_cli = raw.get("sjtu_cli", {})
    mail = raw.get("mail", {})
    shuiyuan = raw.get("shuiyuan", {})
    notify = raw.get("notify", {})
    scheduler = raw.get("scheduler", {})
    llm_raw = raw.get("llm", {})
    # 空 dict → LLMConfig() (mode=off)
    llm_cfg = LLMConfig.model_validate(llm_raw) if llm_raw else LLMConfig()

    return Config(
        sjtu_binary=sjtu_cli.get("binary", _DEFAULTS_NON_LLM["sjtu_binary"]),
        timeout_seconds=int(sjtu_cli.get("timeout_seconds", _DEFAULTS_NON_LLM["timeout_seconds"])),
        mail_limit=int(mail.get("limit", _DEFAULTS_NON_LLM["mail_limit"])),
        shuiyuan_limit=int(shuiyuan.get("limit", _DEFAULTS_NON_LLM["shuiyuan_limit"])),
        notify_app_name=notify.get("app_name", _DEFAULTS_NON_LLM["notify_app_name"]),
        min_interval_hours=int(scheduler.get("min_interval_hours", _DEFAULTS_NON_LLM["min_interval_hours"])),
        llm=llm_cfg,
    )
```

- [ ] **Step 5: 跑全测**

```powershell
pytest tests/test_config.py -v
```

Expected: 旧 2 + 新 6 = 8 passed。

- [ ] **Step 6: 全测**

```powershell
pytest -v
```

Expected: 全部 pass。

- [ ] **Step 7: Commit**

```powershell
git add src/sjtu_daily/config.py tests/test_config.py .gitignore
git commit -m "feat: Config 加 llm: LLMConfig 字段 + pydantic 校验 + gitignore guard"
```

---

## Task 9: 改 `state.py` 加 llm_runs 表 + record_llm_run

**Files:**
- Modify: `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\state.py`
- Modify: `C:\Users\<your-username>\sjtu-daily\tests\test_state.py`

- [ ] **Step 1: 追加测试到 `tests/test_state.py`**

文件 `C:\Users\<your-username>\sjtu-daily\tests\test_state.py` 末尾追加：

```python
# ============== v1.1 llm_runs 表 ==============

def test_llm_runs_table_schema(tmp_path):
    """red line 6: llm_runs 表恰好 9 列，禁任何 PII 列（prompt/response 文本永不入库）。"""
    db = StateDB(tmp_path / "state.db")
    db.init()
    cols = db.list_columns("llm_runs")
    # 主键 id + 8 业务列 = 9
    expected = {"id", "run_at", "provider", "mode", "latency_ms",
                "tokens_in", "tokens_out", "cost_usd", "ok", "error"}
    # error 列也算 → 10
    assert set(cols) == expected, f"llm_runs schema 不对: {cols}"
    # 禁列守门
    forbidden = {"prompt", "response", "content", "body", "subject", "from", "title", "api_key"}
    assert not (forbidden & set(cols))


def test_record_llm_run_inserts_row(tmp_path):
    from sjtu_daily.llm.base import RunMeta

    db = StateDB(tmp_path / "state.db")
    db.init()
    meta = RunMeta(
        provider="gemini", model="gemini-2.5-flash-lite", mode="analyze",
        latency_ms=1234, tokens_in=500, tokens_out=200,
        cost_usd=0.000280, ok=True,
    )
    db.record_llm_run(meta)
    rows = db.list_llm_runs(limit=10)
    assert len(rows) == 1
    r = rows[0]
    assert r["provider"] == "gemini"
    assert r["mode"] == "analyze"
    assert r["latency_ms"] == 1234
    assert r["tokens_in"] == 500
    assert r["tokens_out"] == 200
    assert abs(r["cost_usd"] - 0.000280) < 1e-9
    assert r["ok"] == 1
    assert r["error"] is None


def test_record_llm_run_with_error(tmp_path):
    from sjtu_daily.llm.base import RunMeta

    db = StateDB(tmp_path / "state.db")
    db.init()
    meta = RunMeta(
        provider="deepseek", model="deepseek-v4-flash", mode="polish",
        latency_ms=15001, tokens_in=0, tokens_out=0, cost_usd=0.0,
        ok=False, error="timeout after 15s",
    )
    db.record_llm_run(meta)
    rows = db.list_llm_runs(limit=10)
    assert rows[0]["ok"] == 0
    assert rows[0]["error"] == "timeout after 15s"


def test_record_llm_run_does_not_store_prompt_or_response(tmp_path):
    """red line 7: 即便有人加了 extra 字段也不能存 prompt/response 文本。"""
    from sjtu_daily.llm.base import RunMeta

    db = StateDB(tmp_path / "state.db")
    db.init()
    meta = RunMeta(
        provider="gemini", model="m", mode="analyze",
        latency_ms=10, tokens_in=1, tokens_out=1, cost_usd=0.0, ok=True,
    )
    db.record_llm_run(meta)
    # 直接 sqlite 看 dump 是否含任何文本字段
    import sqlite3
    con = sqlite3.connect(tmp_path / "state.db")
    rows = con.execute("SELECT * FROM llm_runs").fetchall()
    con.close()
    # 每行就是 (id, run_at, provider, model, mode, latency_ms, tokens_in, tokens_out, cost_usd, ok, error)
    # 共 11 列（id + run_at + 9 业务），所有列我们都知道是什么
    assert len(rows[0]) == 11


def test_record_llm_run_multiple_rows(tmp_path):
    from sjtu_daily.llm.base import RunMeta

    db = StateDB(tmp_path / "state.db")
    db.init()
    for i in range(3):
        meta = RunMeta(
            provider="gemini", model="m", mode="analyze",
            latency_ms=10 + i, tokens_in=1, tokens_out=1,
            cost_usd=0.0001 * (i + 1), ok=True,
        )
        db.record_llm_run(meta)
    rows = db.list_llm_runs(limit=10)
    assert len(rows) == 3
    # 默认按 run_at desc，但 3 个时间戳都很接近：检 sum
    total = sum(r["cost_usd"] for r in rows)
    assert abs(total - (0.0001 + 0.0002 + 0.0003)) < 1e-9


def test_list_llm_runs_limit(tmp_path):
    from sjtu_daily.llm.base import RunMeta

    db = StateDB(tmp_path / "state.db")
    db.init()
    for i in range(10):
        db.record_llm_run(RunMeta(
            provider="gemini", model="m", mode="analyze",
            latency_ms=10, tokens_in=1, tokens_out=1, cost_usd=0.0001, ok=True,
        ))
    rows = db.list_llm_runs(limit=3)
    assert len(rows) == 3


def test_total_llm_cost_usd(tmp_path):
    """成本累计聚合便于 README 估算。"""
    from sjtu_daily.llm.base import RunMeta

    db = StateDB(tmp_path / "state.db")
    db.init()
    for c in [0.0001, 0.0002, 0.0003]:
        db.record_llm_run(RunMeta(
            provider="gemini", model="m", mode="analyze",
            latency_ms=10, tokens_in=1, tokens_out=1, cost_usd=c, ok=True,
        ))
    total = db.total_llm_cost_usd()
    assert abs(total - 0.0006) < 1e-9
```

- [ ] **Step 2: 跑测试确认 FAIL**

```powershell
pytest tests/test_state.py -v
```

Expected: 旧 ~10 个 pass，新 7 个 FAIL（`AttributeError: 'StateDB' has no 'record_llm_run'`）。

- [ ] **Step 3: 改 `state.py`**

打开 `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\state.py`，在 `_ALLOWED_META_COLUMNS = ...` 下方加：

```python
_ALLOWED_LLM_RUNS_COLUMNS = frozenset({
    "id", "run_at", "provider", "model", "mode",
    "latency_ms", "tokens_in", "tokens_out", "cost_usd", "ok", "error",
})

_SCHEMA_LLM_RUNS = """
CREATE TABLE IF NOT EXISTS llm_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_at TEXT NOT NULL,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    mode TEXT NOT NULL,
    latency_ms INTEGER NOT NULL,
    tokens_in INTEGER NOT NULL,
    tokens_out INTEGER NOT NULL,
    cost_usd REAL NOT NULL,
    ok INTEGER NOT NULL,
    error TEXT
);
"""
```

然后改 `list_columns` 白名单（加 `"llm_runs"`）：

```python
    def list_columns(self, table: str) -> list[str]:
        if table not in ("seen", "meta", "llm_runs"):
            raise ValueError(f"不允许查询的 table: {table}")
        with self._connect() as con:
            cur = con.execute(f"PRAGMA table_info({table})")
            return [row[1] for row in cur.fetchall()]
```

改 `init()` 加新表 + 守门：

```python
    def init(self) -> None:
        """建表（若不存在）+ 守门校验 schema。"""
        with self._connect() as con:
            con.executescript(_SCHEMA_SEEN + _SCHEMA_META + _SCHEMA_LLM_RUNS)
        # 校验 seen
        seen_cols = set(self.list_columns("seen"))
        if seen_cols != _ALLOWED_SEEN_COLUMNS:
            extra = seen_cols - _ALLOWED_SEEN_COLUMNS
            raise RuntimeError(
                f"seen 表含禁列（PII 红线 5）: {extra}。删除 {self.path} 后重建。"
            )
        # 校验 meta
        meta_cols = set(self.list_columns("meta"))
        if meta_cols != _ALLOWED_META_COLUMNS:
            extra = meta_cols - _ALLOWED_META_COLUMNS
            raise RuntimeError(f"meta 表含禁列: {extra}")
        # 校验 llm_runs（v1.1 red line 6+7）
        llm_cols = set(self.list_columns("llm_runs"))
        if llm_cols != _ALLOWED_LLM_RUNS_COLUMNS:
            extra = llm_cols - _ALLOWED_LLM_RUNS_COLUMNS
            raise RuntimeError(
                f"llm_runs 表含禁列（PII / prompt 红线 7）: {extra}。删除 {self.path} 后重建。"
            )
```

在 `StateDB` 类末尾追加方法：

```python
    def record_llm_run(self, meta) -> None:
        """写一行 llm_runs。meta 必须是 RunMeta（duck typing 即可）。

        red line 7: 永不存 prompt / response 文本 —— 本方法签名只接 meta，连
        prompt 参数都不接收。
        """
        now = datetime.now(timezone.utc).isoformat()
        with self._connect() as con:
            con.execute(
                """
                INSERT INTO llm_runs
                (run_at, provider, model, mode, latency_ms, tokens_in, tokens_out, cost_usd, ok, error)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                [
                    now, meta.provider, meta.model, meta.mode,
                    int(meta.latency_ms), int(meta.tokens_in), int(meta.tokens_out),
                    float(meta.cost_usd), 1 if meta.ok else 0, meta.error,
                ],
            )
            con.commit()

    def list_llm_runs(self, *, limit: int = 100) -> list[dict]:
        """最近 N 条 llm_runs。"""
        with self._connect() as con:
            con.row_factory = sqlite3.Row
            rows = con.execute(
                "SELECT * FROM llm_runs ORDER BY id DESC LIMIT ?",
                [limit],
            ).fetchall()
        return [dict(r) for r in rows]

    def total_llm_cost_usd(self) -> float:
        """累计 LLM 成本（USD）。"""
        with self._connect() as con:
            row = con.execute(
                "SELECT COALESCE(SUM(cost_usd), 0.0) FROM llm_runs"
            ).fetchone()
        return float(row[0])
```

- [ ] **Step 4: 跑测试确认 PASS**

```powershell
pytest tests/test_state.py -v
```

Expected: 旧 ~10 + 新 7 = 17 passed。

- [ ] **Step 5: 行数检查**

```powershell
(Get-Content src/sjtu_daily/state.py | Measure-Object -Line).Lines
```

Expected: ≤ 200 行。如果超了拆 `state.py` 和 `state_llm.py`。

- [ ] **Step 6: 全测**

```powershell
pytest -v
```

Expected: 全部 pass。

- [ ] **Step 7: Commit**

```powershell
git add src/sjtu_daily/state.py tests/test_state.py
git commit -m "feat(state): llm_runs 表 + record_llm_run + 守门（red line 6/7）"
```

---

## Task 10: 改 `render.py` + 模板加 summary section

**Files:**
- Modify: `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\render.py`
- Modify: `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\templates\dashboard.html.j2`
- Modify: `C:\Users\<your-username>\sjtu-daily\tests\test_render.py`

- [ ] **Step 1: 追加测试到 `tests/test_render.py`**

文件 `C:\Users\<your-username>\sjtu-daily\tests\test_render.py` 末尾追加：

```python
# ============== v1.1 LLM summary 渲染 ==============

from sjtu_daily.llm.base import SummaryResult


def _summary_basic() -> SummaryResult:
    return SummaryResult(
        today_highlights_text="今天有 3 件事要处理",
        urgent_text="紧急：1 封邮件待回",
        suggestions_text="建议：先回邮件再开会",
        punchline="3 邮件 1 待办",
        runs=[],
    )


def test_render_no_summary_section_when_summary_is_none():
    """summary=None 时 dashboard 顶部不渲染 summary 区块（v1 兼容）。"""
    snap = _make_snapshot()
    new_ids = {k: set() for k in ["mail", "messages", "services", "shuiyuan", "card"]}
    html = render_dashboard(snap, new_ids, summary=None)
    assert "llm-summary" not in html
    assert "今日重点" not in html  # summary 段才有"今日重点"标题
    # 但 5 个原有 section 仍在
    assert "邮箱未读" in html


def test_render_with_summary_shows_three_sections():
    snap = _make_snapshot()
    new_ids = {k: set() for k in ["mail", "messages", "services", "shuiyuan", "card"]}
    html = render_dashboard(snap, new_ids, summary=_summary_basic())
    assert "llm-summary" in html
    assert "今日重点" in html or "📌" in html
    assert "紧急" in html or "⚠️" in html
    assert "建议" in html or "💡" in html
    assert "今天有 3 件事要处理" in html
    assert "1 封邮件待回" in html


def test_render_escapes_html_in_summary_text():
    """red line 3: LLM 输出当 plaintext，HTML 标签必须 escape。"""
    malicious = SummaryResult(
        today_highlights_text="<script>alert(1)</script>",
        urgent_text="<img src=x onerror=alert(1)>",
        suggestions_text="text & < >",
        punchline="<b>bold</b>",
        runs=[],
    )
    snap = _make_snapshot()
    new_ids = {k: set() for k in ["mail", "messages", "services", "shuiyuan", "card"]}
    html = render_dashboard(snap, new_ids, summary=malicious)
    # 原始 HTML 必须不出现
    assert "<script>alert(1)</script>" not in html
    assert "<img src=x onerror" not in html
    # escape 形式应该出现
    assert "&lt;script&gt;" in html or "&amp;lt;script&amp;gt;" in html


def test_render_summary_with_empty_strings():
    """空字符串 summary：渲染容错。"""
    empty = SummaryResult(
        today_highlights_text="",
        urgent_text="",
        suggestions_text="",
        punchline="",
        runs=[],
    )
    snap = _make_snapshot()
    new_ids = {k: set() for k in ["mail", "messages", "services", "shuiyuan", "card"]}
    html = render_dashboard(snap, new_ids, summary=empty)
    # 不崩
    assert "SJTU 今日" in html
```

注意：旧测试 `test_render_contains_titles` 等都不传 `summary` 参数，函数签名要支持默认值 `summary=None`。

- [ ] **Step 2: 跑测试确认 FAIL**

```powershell
pytest tests/test_render.py -v
```

Expected: 旧 7 个 pass（向后兼容），新 4 个 FAIL（模板没有 summary 区块）。

- [ ] **Step 3: 改 `templates/dashboard.html.j2` 顶部加 summary 区块**

打开 `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\templates\dashboard.html.j2`，在 `<style>` 段末尾（`}` 之前）追加 CSS：

```html
  .llm-summary { background: #f0f7ff; border: 1px solid #d6e4ff; border-radius: 8px; padding: 14px 18px; margin-bottom: 24px; }
  .llm-summary h2 { font-size: 16px; margin: 0 0 12px; color: #1d39c4; }
  .llm-summary .sub-section { margin-bottom: 10px; }
  .llm-summary .sub-section:last-child { margin-bottom: 0; }
  .llm-summary .sub-section .label { font-weight: bold; margin-right: 6px; color: #0050b3; }
  .llm-summary .sub-section .body { white-space: pre-wrap; }
```

然后在 `<div class="meta">...</div>` 行**之后**、 `{# ============== mail ============== #}` **之前**插入：

```html
{# ============== v1.1 LLM summary（仅当 summary 不为 None 时渲染） ============== #}
{% if summary %}
<section class="llm-summary">
  <h2>🤖 今日摘要</h2>
  {% if summary.today_highlights_text %}
  <div class="sub-section">
    <span class="label">📌 今日重点</span>
    <span class="body">{{ summary.today_highlights_text }}</span>
  </div>
  {% endif %}
  {% if summary.urgent_text %}
  <div class="sub-section">
    <span class="label">⚠️ 紧急</span>
    <span class="body">{{ summary.urgent_text }}</span>
  </div>
  {% endif %}
  {% if summary.suggestions_text %}
  <div class="sub-section">
    <span class="label">💡 建议</span>
    <span class="body">{{ summary.suggestions_text }}</span>
  </div>
  {% endif %}
</section>
{% endif %}
```

- [ ] **Step 4: 改 `render.py` 签名加 summary 参数**

打开 `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\render.py`，把 `render_dashboard` 改为：

```python
"""Jinja2 → dashboard.html。

red line 6：模板只引用白名单字段，绝不引用 PII。
red line 3 (v1.1)：summary.*_text / punchline 是 plaintext，由 Jinja2 autoescape
兜底。模板里全部走 `{{ }}` 默认转义，不用 `| safe`。
"""
from __future__ import annotations

from datetime import datetime, timezone
from pathlib import Path

from jinja2 import Environment, FileSystemLoader, select_autoescape

from sjtu_daily.llm.base import SummaryResult
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
    snap: Snapshot,
    new_ids: dict[str, set[str]],
    *,
    now: datetime | None = None,
    summary: SummaryResult | None = None,
) -> str:
    """渲染 dashboard.html 内容。

    Args:
        snap: 5 子系统快照
        new_ids: 每个 category 中标"新增"的 item_id 集合
        now: 渲染时间戳（默认 datetime.now()）
        summary: 可选 LLM 摘要；None 时不渲染顶部 summary 区块（v1.1 fallback 兜底）
    """
    env = _env()
    tmpl = env.get_template("dashboard.html.j2")
    return tmpl.render(
        snap=snap.results,
        new_ids=new_ids,
        generated_at=(now or datetime.now()).strftime("%Y-%m-%d %H:%M:%S"),
        has_auth_required=snap.has_any_auth_required,
        summary=summary,
    )
```

- [ ] **Step 5: 跑测试确认 PASS**

```powershell
pytest tests/test_render.py -v
```

Expected: 旧 7 + 新 4 = 11 passed。

- [ ] **Step 6: 全测**

```powershell
pytest -v
```

Expected: 全部 pass。

- [ ] **Step 7: Commit**

```powershell
git add src/sjtu_daily/render.py src/sjtu_daily/templates/dashboard.html.j2 tests/test_render.py
git commit -m "feat(render): dashboard 顶部 LLM summary section + Jinja2 autoescape 防 XSS"
```

---

## Task 11: 改 `notify.py` 加 punchline 参数

**Files:**
- Modify: `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\notify.py`
- Modify: `C:\Users\<your-username>\sjtu-daily\tests\test_notify.py`

- [ ] **Step 1: 追加测试到 `tests/test_notify.py`**

文件 `C:\Users\<your-username>\sjtu-daily\tests\test_notify.py` 末尾追加：

```python
# ============== v1.1 punchline 优先 ==============


def test_send_toast_uses_punchline_when_provided(mocker):
    """传 punchline 时 body 用 punchline，而不是 v1 的计数拼装。"""
    fake_toaster = mocker.MagicMock()
    mocker.patch("sjtu_daily.notify._make_toaster", return_value=fake_toaster)
    captured = {}

    def _capture(title, body, url):
        captured["title"] = title
        captured["body"] = body
        from unittest.mock import MagicMock as MM
        return MM()
    mocker.patch("sjtu_daily.notify._make_toast", side_effect=_capture)

    sent = send_summary_toast(
        new_counts={"mail": 2, "messages": 0, "services": 0, "shuiyuan": 0, "card": 0},
        auth_required=False,
        dashboard_url="file:///x",
        app_name="Test",
        punchline="今天要回 2 封邮件",
    )
    assert sent is True
    assert captured["body"] == "今天要回 2 封邮件"


def test_send_toast_falls_back_to_v1_body_when_no_punchline(mocker):
    """punchline=None 时回退 v1 行为（"邮件 2" 格式）。"""
    fake_toaster = mocker.MagicMock()
    mocker.patch("sjtu_daily.notify._make_toaster", return_value=fake_toaster)
    captured = {}

    def _capture(title, body, url):
        captured["title"] = title
        captured["body"] = body
        from unittest.mock import MagicMock as MM
        return MM()
    mocker.patch("sjtu_daily.notify._make_toast", side_effect=_capture)

    sent = send_summary_toast(
        new_counts={"mail": 2, "messages": 1, "services": 0, "shuiyuan": 0, "card": 0},
        auth_required=False,
        dashboard_url="file:///x",
        app_name="Test",
        punchline=None,
    )
    assert sent is True
    assert "邮件 2" in captured["body"]
    assert "消息 1" in captured["body"]


def test_send_toast_falls_back_when_punchline_empty(mocker):
    """空字符串 punchline 也回退 v1（LLM 兜底可能给空）。"""
    fake_toaster = mocker.MagicMock()
    mocker.patch("sjtu_daily.notify._make_toaster", return_value=fake_toaster)
    captured = {}
    mocker.patch("sjtu_daily.notify._make_toast",
                 side_effect=lambda t, b, u: captured.update(body=b) or mocker.MagicMock())

    sent = send_summary_toast(
        new_counts={"mail": 2, "messages": 0, "services": 0, "shuiyuan": 0, "card": 0},
        auth_required=False,
        dashboard_url="file:///x",
        app_name="Test",
        punchline="",
    )
    assert sent is True
    assert "邮件 2" in captured["body"]


def test_send_toast_punchline_does_not_override_auth_warning(mocker):
    """auth_required 时仍用 v1 的 "session 过期" body，不被 punchline 覆盖。"""
    fake_toaster = mocker.MagicMock()
    mocker.patch("sjtu_daily.notify._make_toaster", return_value=fake_toaster)
    captured = {}
    mocker.patch("sjtu_daily.notify._make_toast",
                 side_effect=lambda t, b, u: captured.update(body=b, title=t) or mocker.MagicMock())

    sent = send_summary_toast(
        new_counts={k: 0 for k in ["mail", "messages", "services", "shuiyuan", "card"]},
        auth_required=True,
        dashboard_url="file:///x",
        app_name="Test",
        punchline="今天没什么事",
    )
    assert sent is True
    # auth 警告优先
    assert "session" in captured["body"].lower() or "过期" in captured["body"] or "login" in captured["body"]
```

- [ ] **Step 2: 跑测试确认 FAIL**

```powershell
pytest tests/test_notify.py -v
```

Expected: 旧 4 pass，新 4 FAIL（`send_summary_toast` 还不接 `punchline`）。

- [ ] **Step 3: 改 `notify.py`**

文件 `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\notify.py`：

```python
"""Windows Toast 包装。Toast 失败永不让主流程崩。

v1.1: send_summary_toast 加 punchline 参数。punchline truthy 时优先用作 body；
auth_required 时仍走 v1 的 "session 过期" 提示（punchline 不覆盖警告）。
"""
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


def _build_v1_body(new_counts: dict[str, int], total_new: int) -> str:
    """v1 的"邮件 N / 消息 M"格式 body。"""
    parts = []
    if new_counts.get("mail", 0):
        parts.append(f"邮件 {new_counts['mail']}")
    if new_counts.get("messages", 0):
        parts.append(f"消息 {new_counts['messages']}")
    if new_counts.get("services", 0):
        parts.append(f"待办 {new_counts['services']}")
    if new_counts.get("shuiyuan", 0):
        parts.append(f"水源 {new_counts['shuiyuan']}")
    return " / ".join(parts) if parts else "查看详情"


def send_summary_toast(
    *,
    new_counts: dict[str, int],
    auth_required: bool,
    dashboard_url: str,
    app_name: str,
    punchline: str | None = None,
) -> bool:
    """发摘要 Toast。返 True = 成功发送；False = 未发（无新增 / 失败）。

    Args:
        punchline: 可选 LLM 生成的一句话（v1.1）。truthy 时优先作 body；
                   auth_required 时仍走警告（punchline 不覆盖）。
    """
    total_new = sum(new_counts.values())
    if total_new == 0 and not auth_required:
        log.info("无新增项 + 无 auth 警告，跳过 Toast")
        return False

    if auth_required:
        title = "⚠️ SJTU session 过期"
        body = "请运行 `sjtu login` 后再跑 sjtu-daily"
    else:
        title = f"SJTU 今日新增 {total_new} 条"
        if punchline:
            body = punchline
        else:
            body = _build_v1_body(new_counts, total_new)

    try:
        toaster = _make_toaster(app_name)
        toast = _make_toast(title, body, dashboard_url)
        toaster.show_toast(toast)
        return True
    except Exception as e:
        log.warning("Toast 发送失败（不阻塞主流程）: %s", e)
        return False
```

- [ ] **Step 4: 跑测试确认 PASS**

```powershell
pytest tests/test_notify.py -v
```

Expected: 旧 4 + 新 4 = 8 passed。

- [ ] **Step 5: 全测**

```powershell
pytest -v
```

Expected: 全部 pass。

- [ ] **Step 6: Commit**

```powershell
git add src/sjtu_daily/notify.py tests/test_notify.py
git commit -m "feat(notify): punchline 优先 + auth_required 仍走警告 + v1 fallback"
```

---

## Task 12: 改 `cli.py` wire summarize + `--no-llm` flag

**Files:**
- Modify: `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\cli.py`
- Modify: `C:\Users\<your-username>\sjtu-daily\tests\test_cli.py`

- [ ] **Step 1: 追加测试到 `tests/test_cli.py`**

文件 `C:\Users\<your-username>\sjtu-daily\tests\test_cli.py` 末尾追加：

```python
# ============== v1.1 LLM wiring ==============

from sjtu_daily.llm.base import SummaryResult


def _good_summary():
    return SummaryResult(
        today_highlights_text="今日重点", urgent_text="紧急",
        suggestions_text="建议", punchline="3 件事",
        runs=[],
    )


def test_main_run_calls_summarize_when_llm_enabled(tmp_path, mocker, monkeypatch):
    """有 config.toml + [llm] mode=cascade 时 cli 调 summarize。"""
    monkeypatch.setenv("SJTU_DAILY_HOME", str(tmp_path))
    (tmp_path / "config.toml").write_text(
        '''[llm]
mode = "cascade"
gemini_api_key = "gk_x"
deepseek_api_key = "dk_x"
''',
        encoding="utf-8",
    )
    mocker.patch("sjtu_daily.cli.run_all", return_value=_good_snapshot())
    summarize_mock = mocker.patch("sjtu_daily.cli.summarize", return_value=_good_summary())
    mocker.patch("sjtu_daily.cli.send_summary_toast", return_value=True)

    rc = main(["run", "--force"])
    assert rc == 0
    summarize_mock.assert_called_once()
    # dashboard 含 LLM 摘要
    html = (tmp_path / "dashboard.html").read_text(encoding="utf-8")
    assert "今日重点" in html


def test_main_run_skips_summarize_when_mode_off(tmp_path, mocker, monkeypatch):
    """mode=off 时 cli 不调 summarize。"""
    monkeypatch.setenv("SJTU_DAILY_HOME", str(tmp_path))
    # 不写 config.toml → LLMConfig() mode=off
    mocker.patch("sjtu_daily.cli.run_all", return_value=_good_snapshot())
    summarize_mock = mocker.patch("sjtu_daily.cli.summarize")
    mocker.patch("sjtu_daily.cli.send_summary_toast", return_value=True)

    rc = main(["run", "--force"])
    assert rc == 0
    summarize_mock.assert_not_called()


def test_main_no_llm_flag_skips_summarize(tmp_path, mocker, monkeypatch):
    """--no-llm 即便 config 启用了 LLM 也跳过。"""
    monkeypatch.setenv("SJTU_DAILY_HOME", str(tmp_path))
    (tmp_path / "config.toml").write_text(
        '''[llm]
mode = "cascade"
gemini_api_key = "gk_x"
deepseek_api_key = "dk_x"
''',
        encoding="utf-8",
    )
    mocker.patch("sjtu_daily.cli.run_all", return_value=_good_snapshot())
    summarize_mock = mocker.patch("sjtu_daily.cli.summarize")
    mocker.patch("sjtu_daily.cli.send_summary_toast", return_value=True)

    rc = main(["run", "--force", "--no-llm"])
    assert rc == 0
    summarize_mock.assert_not_called()


def test_main_run_continues_when_summarize_returns_none(tmp_path, mocker, monkeypatch):
    """summarize 返 None 时主流程继续，dashboard 不含 summary 区块，exit 0。"""
    monkeypatch.setenv("SJTU_DAILY_HOME", str(tmp_path))
    (tmp_path / "config.toml").write_text(
        '''[llm]
mode = "cascade"
gemini_api_key = "gk_x"
deepseek_api_key = "dk_x"
''',
        encoding="utf-8",
    )
    mocker.patch("sjtu_daily.cli.run_all", return_value=_good_snapshot())
    mocker.patch("sjtu_daily.cli.summarize", return_value=None)
    mocker.patch("sjtu_daily.cli.send_summary_toast", return_value=True)

    rc = main(["run", "--force"])
    assert rc == 0
    html = (tmp_path / "dashboard.html").read_text(encoding="utf-8")
    assert "llm-summary" not in html


def test_main_run_continues_when_summarize_raises(tmp_path, mocker, monkeypatch):
    """red line 4: summarize 抛异常时 cli 捕获 + log + summary=None 继续，exit 0。"""
    monkeypatch.setenv("SJTU_DAILY_HOME", str(tmp_path))
    (tmp_path / "config.toml").write_text(
        '''[llm]
mode = "cascade"
gemini_api_key = "gk_x"
deepseek_api_key = "dk_x"
''',
        encoding="utf-8",
    )
    mocker.patch("sjtu_daily.cli.run_all", return_value=_good_snapshot())
    mocker.patch("sjtu_daily.cli.summarize", side_effect=RuntimeError("boom"))
    mocker.patch("sjtu_daily.cli.send_summary_toast", return_value=True)

    rc = main(["run", "--force"])
    assert rc == 0  # 主流程不崩
    assert (tmp_path / "dashboard.html").is_file()


def test_main_dry_run_still_calls_summarize(tmp_path, mocker, monkeypatch):
    """dry-run 也调 LLM（记成本 + 看效果），但不写 state.db / 不发 Toast。"""
    monkeypatch.setenv("SJTU_DAILY_HOME", str(tmp_path))
    (tmp_path / "config.toml").write_text(
        '''[llm]
mode = "cascade"
gemini_api_key = "gk_x"
deepseek_api_key = "dk_x"
''',
        encoding="utf-8",
    )
    mocker.patch("sjtu_daily.cli.run_all", return_value=_good_snapshot())
    summarize_mock = mocker.patch("sjtu_daily.cli.summarize", return_value=_good_summary())
    toast_mock = mocker.patch("sjtu_daily.cli.send_summary_toast")

    rc = main(["dry-run"])
    assert rc == 0
    summarize_mock.assert_called_once()
    toast_mock.assert_not_called()


def test_main_run_passes_punchline_to_toast(tmp_path, mocker, monkeypatch):
    """summary.punchline 通过参数传到 send_summary_toast。"""
    monkeypatch.setenv("SJTU_DAILY_HOME", str(tmp_path))
    (tmp_path / "config.toml").write_text(
        '''[llm]
mode = "cascade"
gemini_api_key = "gk_x"
deepseek_api_key = "dk_x"
''',
        encoding="utf-8",
    )
    mocker.patch("sjtu_daily.cli.run_all", return_value=_good_snapshot())
    mocker.patch("sjtu_daily.cli.summarize", return_value=_good_summary())
    toast_mock = mocker.patch("sjtu_daily.cli.send_summary_toast", return_value=True)

    main(["run", "--force"])
    call_kwargs = toast_mock.call_args.kwargs
    assert call_kwargs.get("punchline") == "3 件事"
```

- [ ] **Step 2: 跑测试确认 FAIL**

```powershell
pytest tests/test_cli.py -v
```

Expected: 旧 6 pass，新 7 FAIL（`summarize` 没接入 cli）。

- [ ] **Step 3: 改 `cli.py`**

文件 `C:\Users\<your-username>\sjtu-daily\src\sjtu_daily\cli.py`：

```python
"""CLI 入口：sjtu-daily {run|dry-run|version}。

v1.1: 接 summarize() 进 _do_run；新增 --no-llm flag；LLM 失败不影响主流程。
"""
from __future__ import annotations

import argparse
import logging
import sys
from datetime import datetime, timezone

from sjtu_daily import __version__, paths
from sjtu_daily.config import load_config
from sjtu_daily.llm import summarize
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
        if cat == "card":
            out[cat] = set()
            continue
        ids = [it["id"] for it in res.items if it.get("id")]
        new_ids = db.diff_new_items(cat, ids)
        out[cat] = set(new_ids)
        if persist and ids:
            db.mark_seen(cat, ids)
    return out


def _try_summarize(snap, llm_cfg, db: StateDB, *, no_llm: bool):
    """安全调 summarize：任何异常都 catch + log，主流程不崩（red line 4）。"""
    if no_llm:
        log.info("--no-llm 已设，跳过 LLM 摘要")
        return None
    if llm_cfg.mode == "off":
        return None
    try:
        return summarize(snap, llm_cfg, db=db)
    except Exception as e:  # noqa: BLE001 — 顶层兜底
        log.warning("LLM 摘要失败（不阻塞主流程，主流程继续）: %s", e)
        return None


def _do_run(*, dry_run: bool, force: bool, no_llm: bool) -> int:
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

    summary = _try_summarize(snap, cfg.llm, db, no_llm=no_llm)

    html = render_dashboard(snap, new_ids, summary=summary)
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
        punchline = summary.punchline if summary else None
        send_summary_toast(
            new_counts=new_counts,
            auth_required=snap.has_any_auth_required,
            dashboard_url=f"file:///{dashboard.as_posix()}",
            app_name=cfg.notify_app_name,
            punchline=punchline,
        )
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="sjtu-daily")
    sub = parser.add_subparsers(dest="cmd", required=True)
    p_run = sub.add_parser("run", help="跑一次完整流程：拉数据 + 写 dashboard + Toast")
    p_run.add_argument("--force", action="store_true", help="跳过 min_interval 检查")
    p_run.add_argument("--no-llm", action="store_true", help="跳过 LLM 摘要（即便 config 启用）")
    p_dry = sub.add_parser("dry-run", help="只跑 + 输出 html 到 stdout，不写 state / 不发 Toast")
    p_dry.add_argument("--no-llm", action="store_true", help="跳过 LLM 摘要")
    sub.add_parser("version", help="打印版本号")

    args = parser.parse_args(argv)

    if args.cmd == "version":
        print(__version__)
        raise SystemExit(0)
    if args.cmd == "run":
        return _do_run(dry_run=False, force=args.force, no_llm=args.no_llm)
    if args.cmd == "dry-run":
        return _do_run(dry_run=True, force=False, no_llm=args.no_llm)
    parser.print_help()
    return 2
```

- [ ] **Step 4: 跑测试确认 PASS**

```powershell
pytest tests/test_cli.py -v
```

Expected: 旧 6 + 新 7 = 13 passed。

- [ ] **Step 5: 全测**

```powershell
pytest -v
```

Expected: 全部 pass（v1 50+ + LLM 63 + 改造增量 ≈ 130+）。

- [ ] **Step 6: 行数检查**

```powershell
(Get-Content src/sjtu_daily/cli.py | Measure-Object -Line).Lines
```

Expected: ≤ 200 行。

- [ ] **Step 7: Commit**

```powershell
git add src/sjtu_daily/cli.py tests/test_cli.py
git commit -m "feat(cli): wire summarize 进 _do_run + --no-llm flag + LLM 失败 swallow（red line 4）"
```

---

## Task 13: 文档 + smoke prep（config.example.toml + README）

**Files:**
- Modify: `C:\Users\<your-username>\sjtu-daily\config.example.toml`
- Modify: `C:\Users\<your-username>\sjtu-daily\README.md`

- [ ] **Step 1: 改 `config.example.toml` 加 `[llm]` 段**

文件 `C:\Users\<your-username>\sjtu-daily\config.example.toml`，末尾追加：

```toml

# ============================================================
# v1.1 LLM 摘要层（可选）
# ============================================================
#
# 复制本文件为 ~/sjtu-daily/config.toml 并填 API key 后启用。
# config.toml 已 .gitignore，API key 永不入 git。
#
# 申请 API key:
#   - Gemini: https://aistudio.google.com/apikey
#   - DeepSeek: https://platform.deepseek.com/api_keys
#
# 成本估算（每天 1 次 run，5 子系统约 1k tokens 上下文）：
#   - cascade: 约 $0.001/day（约 ¥0.007/天）
#   - gemini_only: 约 $0.0005/day
#   - deepseek_only: 约 $0.0008/day
# 累计成本可查 ~/.sjtu-daily/data/state.db 的 llm_runs 表。
#
# 失败行为：
#   - LLM 任一步失败：dashboard 顶部不渲染摘要，主流程正常出 5 section
#   - Gemini 失败 → DeepSeek 单跑 fallback
#   - DeepSeek 失败 → 用 Gemini 输出裸拼
#   - 两个都挂 → 退化为 v1 行为（dashboard 仍正常）

[llm]
# 模式：off / cascade / gemini_only / deepseek_only
# off (默认) = 不调 LLM，行为完全等同 v1
mode = "off"

# Gemini API key（从 https://aistudio.google.com/apikey）
gemini_api_key = ""
gemini_model = "gemini-2.5-flash-lite"

# DeepSeek API key（从 https://platform.deepseek.com/api_keys）
deepseek_api_key = ""
deepseek_model = "deepseek-v4-flash"

# 单次 LLM call 超时（秒）
timeout_seconds = 15

# 整个 pipeline 累计预算（秒）
pipeline_budget_seconds = 45

# 任一 provider 失败时是否走 fallback
fallback_on_error = true

# LLM 输出 max tokens（限制成本）
max_tokens_out = 800
```

- [ ] **Step 2: 改 `README.md` 加 v1.1 章节**

打开 `C:\Users\<your-username>\sjtu-daily\README.md`，在"## v1 范围"段**之后**插入新章节：

```markdown
## v1.1 LLM 摘要层（可选）

v1.1 在 dashboard 顶部加 3 段中文摘要（📌 今日重点 / ⚠️ 紧急 / 💡 建议），Toast body 也用 LLM 一句话。**完全可选**，不配置 API key 时行为等同 v1。

### 双 LLM cascade

1. **Gemini 2.5 Flash-Lite** 出结构化 JSON 优先级分析（urgent / today_highlights / suggestions / cross_cutting）
2. **DeepSeek-V4-Flash** 接 JSON 产中文文案 + Toast punchline

### Fallback 行为

| 路径 | 行为 |
|------|------|
| Gemini OK + DeepSeek OK | 正向 cascade，最完整 |
| Gemini 失败 | DeepSeek 单跑（合并 prompt）|
| Gemini OK + DeepSeek 失败 | 用 Gemini 原始 list 裸拼（粗糙但能看）|
| 两个都失败 | dashboard 不渲染摘要，行为等同 v1（**dashboard 永远不会因为 LLM 而崩**）|

### 启用步骤

1. 申请 API key：
   - Gemini：https://aistudio.google.com/apikey
   - DeepSeek：https://platform.deepseek.com/api_keys
2. 复制 `config.example.toml` 到 `~/sjtu-daily/config.toml`，在 `[llm]` 段填 key + 改 `mode = "cascade"`
3. **API key 安全**：`config.toml` 已在 `.gitignore`，永不入 git；日志里只露前 8 位 + `***`
4. 跑一次：`sjtu-daily run --force`
5. 想临时跳过 LLM：`sjtu-daily run --force --no-llm`

### 成本

每天 1 次 run，5 子系统约 1k tokens 上下文：

| 模式 | 估算 |
|------|------|
| cascade | 约 $0.001/day（约 ¥0.007/天 / ¥2/年）|
| gemini_only | 约 $0.0005/day |
| deepseek_only | 约 $0.0008/day |

累计成本可查 `~/.sjtu-daily/data/state.db` 的 `llm_runs` 表（无 prompt / response 文本，仅 token count + cost 元数据）。

### 红线

1. **API key 永不入 git**：`config.toml` 已 .gitignore
2. **prompt 永不带 PII**：只发标题 / 时间 / 计数，**不发**邮箱地址 / 正文片段 / 学号
3. **LLM 输出当 plaintext**：Jinja2 autoescape 防 XSS
4. **LLM 失败 ≠ dashboard 失败**：summarize 抛任何异常都被 cli 顶层 catch
5. **超时硬限**：单 call 15s / pipeline 总预算 45s
6. **state.db 永不存 prompt / response 文本**：只记 metadata
```

- [ ] **Step 3: 验证 README 渲染**

```powershell
cd C:\Users\<your-username>\sjtu-daily
# 用任意 markdown 预览器（VSCode / browser）打开
# 或简单检查
Get-Content README.md | Select-String "v1.1"
```

Expected: 至少 1 行匹配。

- [ ] **Step 4: 全测 + 行数最终审计**

```powershell
pytest -v

Get-ChildItem src/sjtu_daily -Recurse -Filter *.py | ForEach-Object {
  $lines = (Get-Content $_.FullName | Measure-Object -Line).Lines
  if ($lines -gt 200) { "WARN $($_.FullName): $lines lines" }
}
Get-ChildItem tests -Recurse -Filter *.py | ForEach-Object {
  $lines = (Get-Content $_.FullName | Measure-Object -Line).Lines
  if ($lines -gt 300) { "WARN $($_.FullName): $lines lines" }
}
```

Expected: 测试全绿；如出现 WARN 立即拆文件再补一个 commit。

- [ ] **Step 5: 冒烟 import（无真 API key）**

```powershell
python -c "from sjtu_daily.llm import summarize, LLMConfig; print(summarize(None, LLMConfig()) if False else 'import ok')"
python -m sjtu_daily version
```

Expected: `import ok`、`0.2.0`。

- [ ] **Step 6: Commit**

```powershell
git add config.example.toml README.md
git commit -m "docs: README v1.1 LLM 章节 + config.example.toml [llm] 段 + 成本估算"
```

---

## 全局验收（v1.1 完工标志）

完工 = 全部 ✅：

- [ ] `pytest -v` 全绿（v1 原 ~50 + LLM 新增 ~80 ≈ 130+ 测试）
- [ ] 每个新 `.py` 文件 ≤ 200 行；每个新测试文件 ≤ 300 行
- [ ] `python -m sjtu_daily version` 输出 `0.2.0`
- [ ] `python -m sjtu_daily run --force --no-llm`（无 config.toml）退出 0 + dashboard.html 出 + 无 summary 区块
- [ ] 在 `config.toml` 配 `[llm] mode = "off"`：行为等同 v1
- [ ] 在 `config.toml` 配 `[llm] mode = "cascade"` + 假 key：summarize 抛 → cli 捕获 → dashboard 仍出 → exit 0（red line 4 真机验证）
- [ ] grep `dashboard.html` 不到 PII（from_address / fragment / excerpt / body_plain）
- [ ] grep `state.db` llm_runs 表只有 11 列（不含任何 prompt / response 文本字段）
- [ ] grep `.gitignore`：`config.toml` 在内
- [ ] `git log --since="1 day ago" --name-only`：sjtu-cli 仓 0 个文件变动（零侵入 v1 老红线）
- [ ] `(Get-Content config.toml -Raw) -match 'gk_|dk_'` 但 `git ls-files` 不含 config.toml：确认 API key 没入 git

---

## Self-Review checklist

**1. Spec coverage:** 14 task 全部对应 spec 文件变更：

| Spec 文件 | Task |
|-----------|------|
| `llm/schemas.py` | Task 1 |
| `llm/base.py` | Task 2 |
| `llm/prompts.py` | Task 3 |
| `llm/gemini.py` | Task 4 |
| `llm/deepseek.py` | Task 5 |
| `llm/pipeline.py` | Task 6 |
| `llm/__init__.py` | Task 7 |
| `config.py` 改 | Task 8 |
| `state.py` 改 | Task 9 |
| `render.py` + template 改 | Task 10 |
| `notify.py` 改 | Task 11 |
| `cli.py` 改 | Task 12 |
| `pyproject.toml` 改 | Task 0 |
| `README.md` + `config.example.toml` 改 | Task 13 |

**2. 红线 8 条覆盖：**

| 红线 | 守护测试 | Task |
|------|----------|------|
| 1 API key 不入 git | `test_config_toml_is_gitignored` + `test_api_key_never_in_repr` ×2 | 4, 5, 8 |
| 2 prompt 无 PII | `test_analyze_input_zero_pii` + `test_fallback_input_zero_pii` | 3 |
| 3 LLM 输出当 plaintext | `test_render_escapes_html_in_summary_text` + prompt 文本守门 | 3, 10 |
| 4 LLM 失败 ≠ dashboard 失败 | `test_main_run_continues_when_summarize_raises` + `test_main_run_continues_when_summarize_returns_none` | 6, 12 |
| 5 超时硬限 | LLMConfig.timeout_seconds 校验 + 单 call 强制传 timeout | 1, 4, 5 |
| 6 成本可观测 | `test_record_llm_run_inserts_row` + `test_total_llm_cost_usd` + `test_summarize_records_every_run_to_db` | 6, 9 |
| 7 不存 prompt / response | `test_llm_runs_table_schema` + `test_record_llm_run_does_not_store_prompt_or_response` | 9 |
| 8 schema gate | `test_summarize_treats_invalid_analysis_as_failure` + `test_analysis_result_caps_*` + `test_analysis_result_rejects_extra_fields` | 1, 6 |

**3. 类型一致性：**

- `LLMConfig`：所有 task 一致用 pydantic v2，frozen + extra=forbid
- `AnalysisResult` / `PolishedResult` / `SummaryResult` / `RunMeta`：跨 task 一致字段名
- `LLMProvider.generate_json` / `generate_text` 签名跨 gemini / deepseek 一致
- `summarize(snap, llm_cfg, *, db=None) → SummaryResult | None` 跨 cli / pipeline 一致
- `send_summary_toast(..., punchline=None)` 跨 cli / notify 一致

**4. 占位 / 模糊语言扫描：** 已检查无 "TBD" / "implement later" / "similar to Task N" / "appropriate error handling"。每 task 每 step 完整代码 + 完整命令 + 完整预期。

---

## 完工后下一步

完工 + 真机冒烟通过 → 把已有 task `#46 用 subagent-driven-development 执行 v1.1 plan` 标 in_progress；执行完毕后 task `#47 真机端到端冒烟 v1.1` 开始（用真 API key 跑 `sjtu-daily run --force` 看 dashboard 顶部 3 段中文是否合理 + Toast punchline 是否生动）。

冒烟脚本（不在本 plan 范围，留给 task #47）：

1. 在 ~/sjtu-daily/config.toml 配 mode=cascade + 两个真 key
2. `sjtu-daily run --force` 观测 logs 是否两次 LLM call 都成功
3. 用浏览器打开 dashboard.html，截图给用户人工 review 3 段文案质量
4. Toast 是否弹出且 body 含 punchline
5. 故意把 gemini_api_key 改成假值，重跑：观测是否走 DeepSeek fallback
6. 两个 key 都改假，重跑：观测是否 dashboard 仍出（无 summary 区块）+ exit 0
7. `python -c "from sjtu_daily.state import StateDB; from sjtu_daily.paths import db_path; print(StateDB(db_path()).total_llm_cost_usd())"` 看累计成本
