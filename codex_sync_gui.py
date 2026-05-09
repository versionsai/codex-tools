from __future__ import annotations

import glob
import json
import shutil
import sqlite3
import threading
from dataclasses import dataclass
from datetime import datetime, timezone
from email.utils import parsedate_to_datetime
from pathlib import Path
from typing import Iterable
from urllib.parse import quote, unquote, urljoin, urlparse
from xml.etree import ElementTree as ET

import requests
from requests.auth import HTTPBasicAuth
import tkinter as tk
from tkinter import messagebox, ttk


CODEX_DIR = Path.home() / ".codex"
CONFIG_PATH = CODEX_DIR / "webdav_sync_config.json"
SYNC_ROOTS = ("sessions", "archived_sessions")
SINGLE_FILES = ("session_index.jsonl",)

BG = "#F4F5F7"
CARD = "#FFFFFF"
TEXT = "#111111"
MUTED = "#6E6E73"
BORDER = "#E6E8EE"
ACCENT = "#0A84FF"
ACCENT_SOFT = "#E8F2FF"
SUCCESS = "#17803D"
WINDOW = "#FBFBFD"
SIDEBAR = "#F7F8FB"
SHADOW = "#EAECF2"
PILL_FILL = "#FFFFFF"
PILL_BORDER = "#D7D7DC"
PILL_HOVER = "#F3F7FF"
PILL_ACCENT = "#C9DBFF"
PILL_SECONDARY_FILL = "#F6F6F8"
PILL_SECONDARY_BORDER = "#DFE0E5"


@dataclass
class RemoteEntry:
    relative_path: str
    is_dir: bool
    last_modified: float | None
    size: int | None


def read_provider(config_toml: Path) -> str:
    for line in config_toml.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line.startswith("model_provider"):
            _, _, raw = line.partition("=")
            provider = raw.strip().strip('"').strip("'")
            if provider:
                return provider
    raise RuntimeError(f"Could not read model_provider from {config_toml}")


def rollout_paths(codex_dir: Path) -> list[Path]:
    return list((codex_dir / "sessions").rglob("rollout-*.jsonl")) + list(
        (codex_dir / "archived_sessions").rglob("rollout-*.jsonl")
    )


def load_existing_index(session_index_path: Path) -> dict[str, dict]:
    items: dict[str, dict] = {}
    if not session_index_path.exists():
        return items
    for line in session_index_path.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        try:
            item = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(item, dict) and item.get("id"):
            items[str(item["id"])] = item
    return items


def rebuild_index(codex_dir: Path, provider: str, logger) -> int:
    existing = load_existing_index(codex_dir / "session_index.jsonl")
    conn = sqlite3.connect(codex_dir / "state_5.sqlite")
    conn.row_factory = sqlite3.Row
    rows = conn.execute(
        """
        SELECT id, title, updated_at, updated_at_ms, cwd, model_provider,
               git_origin_url, git_branch, archived
        FROM threads
        ORDER BY COALESCE(updated_at_ms, updated_at * 1000) DESC, id DESC
        """
    ).fetchall()
    conn.close()

    out: list[str] = []
    for row in rows:
        item = existing.get(row["id"], {})
        cwd = row["cwd"]
        updated_at_ms = row["updated_at_ms"] or row["updated_at"] * 1000
        project_root = str(cwd).lower() if cwd else ""
        project_name = Path(cwd).name if cwd else ""
        item.update(
            {
                "id": row["id"],
                "thread_name": row["title"],
                "updated_at": updated_at_ms,
                "cwd": cwd,
                "model_provider": provider,
                "git_origin_url": row["git_origin_url"],
                "git_branch": row["git_branch"],
                "project_root": item.get("project_root", project_root),
                "project_name": item.get("project_name", project_name),
                "project_key": item.get("project_key", project_root),
            }
        )
        out.append(json.dumps(item, ensure_ascii=False, separators=(",", ":")))

    session_index_path = codex_dir / "session_index.jsonl"
    session_index_path.write_text("\n".join(out) + "\n", encoding="utf-8")
    logger(f"已重建索引，共 {len(out)} 条线程")
    return len(out)


def unify_provider(codex_dir: Path, logger) -> dict:
    provider = read_provider(codex_dir / "config.toml")
    backup_dir = codex_dir / "manual_backups" / f"provider-unifier-{datetime.now().strftime('%Y%m%d-%H%M%S')}"
    backup_dir.mkdir(parents=True, exist_ok=True)
    session_index_path = codex_dir / "session_index.jsonl"
    if session_index_path.exists():
        shutil.copy2(session_index_path, backup_dir / "session_index.jsonl")
    if (codex_dir / "state_5.sqlite").exists():
        src = sqlite3.connect(codex_dir / "state_5.sqlite")
        dst = sqlite3.connect(backup_dir / "state_5.sqlite")
        with dst:
            src.backup(dst)
        dst.close()
        src.close()

    rollout_changed = 0
    for path in rollout_paths(codex_dir):
        changed = False
        out: list[str] = []
        for line in path.read_text(encoding="utf-8").splitlines():
            if not line.strip():
                out.append(line)
                continue
            try:
                item = json.loads(line)
            except json.JSONDecodeError:
                out.append(line)
                continue
            if isinstance(item, dict) and item.get("type") == "session_meta":
                payload = item.get("payload") or {}
                if isinstance(payload, dict) and payload.get("model_provider") != provider:
                    payload["model_provider"] = provider
                    item["payload"] = payload
                    line = json.dumps(item, ensure_ascii=False, separators=(",", ":"))
                    changed = True
            out.append(line)
        if changed:
            path.write_text("\n".join(out) + "\n", encoding="utf-8")
            rollout_changed += 1

    conn = sqlite3.connect(codex_dir / "state_5.sqlite")
    with conn:
        conn.execute(
            "UPDATE threads SET model_provider = ? WHERE model_provider != ?",
            (provider, provider),
        )
    thread_rows_updated = conn.total_changes
    conn.close()

    index_entries = rebuild_index(codex_dir, provider, logger)
    return {
        "provider": provider,
        "backup_dir": str(backup_dir),
        "rollout_changed": rollout_changed,
        "thread_rows_updated": thread_rows_updated,
        "session_index_entries": index_entries,
    }


def encode_relative_path(relative_path: str) -> str:
    parts = [quote(part) for part in relative_path.replace("\\", "/").split("/") if part]
    return "/".join(parts)


def relative_url(base_url: str, relative_path: str) -> str:
    return urljoin(base_url, encode_relative_path(relative_path))


def propfind(session: requests.Session, url: str, depth: str = "1") -> requests.Response:
    body = """<?xml version="1.0" encoding="utf-8" ?>
<d:propfind xmlns:d="DAV:">
  <d:prop>
    <d:displayname />
    <d:resourcetype />
    <d:getcontentlength />
    <d:getlastmodified />
  </d:prop>
</d:propfind>
"""
    response = session.request(
        "PROPFIND",
        url,
        headers={"Depth": depth, "Content-Type": "application/xml; charset=utf-8"},
        data=body.encode("utf-8"),
        timeout=30,
    )
    response.raise_for_status()
    return response


def parse_last_modified(value: str | None) -> float | None:
    if not value:
        return None
    try:
        dt = parsedate_to_datetime(value)
        if dt.tzinfo is None:
            dt = dt.replace(tzinfo=timezone.utc)
        return dt.timestamp()
    except Exception:
        return None


def parse_multistatus(base_url: str, xml_text: str, target_relative: str) -> list[RemoteEntry]:
    root = ET.fromstring(xml_text)
    namespace = {"d": "DAV:"}
    base_path = urlparse(base_url).path.rstrip("/")
    target_path = f"{base_path}/{target_relative.strip('/')}" if target_relative.strip("/") else base_path
    results: list[RemoteEntry] = []
    for response in root.findall("d:response", namespace):
        href_text = response.findtext("d:href", default="", namespaces=namespace)
        href_path = unquote(urlparse(href_text).path).rstrip("/")
        if href_path == target_path.rstrip("/"):
            continue
        relative = href_path.removeprefix(base_path).lstrip("/")
        if not relative:
            continue
        prop = response.find(".//d:prop", namespace)
        if prop is None:
            continue
        resource_type = prop.find("d:resourcetype", namespace)
        is_dir = resource_type is not None and resource_type.find("d:collection", namespace) is not None
        last_modified = parse_last_modified(prop.findtext("d:getlastmodified", default="", namespaces=namespace))
        size_text = prop.findtext("d:getcontentlength", default="", namespaces=namespace).strip()
        size = int(size_text) if size_text.isdigit() else None
        results.append(RemoteEntry(relative, is_dir, last_modified, size))
    return results


def ensure_remote_dir(session: requests.Session, base_url: str, relative_dir: str) -> None:
    normalized_parts = [part for part in relative_dir.replace("\\", "/").split("/") if part]
    current = ""
    for part in normalized_parts:
        current = f"{current}/{part}" if current else part
        url = relative_url(base_url, current).rstrip("/") + "/"
        response = session.request("MKCOL", url, timeout=30)
        if response.status_code not in (201, 301, 405):
            response.raise_for_status()


def list_remote_tree(session: requests.Session, base_url: str, relative_root: str) -> list[RemoteEntry]:
    url = relative_url(base_url, relative_root).rstrip("/") + "/"
    response = propfind(session, url, depth="1")
    direct_entries = parse_multistatus(base_url, response.text, relative_root)
    results: list[RemoteEntry] = []
    for entry in direct_entries:
        results.append(entry)
        if entry.is_dir:
            results.extend(list_remote_tree(session, base_url, entry.relative_path))
    return results


def remote_file_map(session: requests.Session, base_url: str) -> dict[str, RemoteEntry]:
    entries: dict[str, RemoteEntry] = {}
    for root_name in SYNC_ROOTS:
        try:
            for entry in list_remote_tree(session, base_url, root_name):
                if not entry.is_dir:
                    entries[entry.relative_path] = entry
        except requests.HTTPError as exc:
            if exc.response is not None and exc.response.status_code == 404:
                continue
            raise
    try:
        response = propfind(session, relative_url(base_url, "session_index.jsonl"), depth="0")
        direct_entries = parse_multistatus(base_url, response.text, "")
        for entry in direct_entries:
            if entry.relative_path == "session_index.jsonl":
                entries[entry.relative_path] = entry
    except requests.HTTPError as exc:
        if exc.response is None or exc.response.status_code != 404:
            raise
    return entries


def iter_local_files(codex_dir: Path) -> Iterable[Path]:
    for root_name in SYNC_ROOTS:
        pattern = str(codex_dir / root_name / "**" / "rollout-*.jsonl")
        for path in glob.glob(pattern, recursive=True):
            yield Path(path)
    for file_name in SINGLE_FILES:
        path = codex_dir / file_name
        if path.exists():
            yield path


def local_relative_path(codex_dir: Path, path: Path) -> str:
    return path.relative_to(codex_dir).as_posix()


def backup_local_file(codex_dir: Path, local_path: Path, reason: str) -> Path | None:
    if not local_path.exists():
        return None
    timestamp = datetime.now().strftime("%Y%m%d-%H%M%S")
    relative = local_relative_path(codex_dir, local_path)
    backup_path = codex_dir / "sync_backups" / reason / f"{timestamp}" / relative
    backup_path.parent.mkdir(parents=True, exist_ok=True)
    backup_path.write_bytes(local_path.read_bytes())
    return backup_path


def create_session(username: str, password: str, verify_tls: bool) -> requests.Session:
    session = requests.Session()
    session.auth = HTTPBasicAuth(username, password)
    session.verify = verify_tls
    return session


def push_to_webdav(codex_dir: Path, base_url: str, username: str, password: str, verify_tls: bool, logger) -> dict:
    session = create_session(username, password, verify_tls)
    local_files = list(iter_local_files(codex_dir))
    uploaded = 0
    skipped = 0
    remote_entries = remote_file_map(session, base_url)
    for path in local_files:
        relative = local_relative_path(codex_dir, path)
        local_mtime = path.stat().st_mtime
        remote = remote_entries.get(relative)
        if remote and remote.last_modified and remote.last_modified >= local_mtime and remote.size == path.stat().st_size:
            skipped += 1
            continue
        ensure_remote_dir(session, base_url, str(Path(relative).parent).replace(".", ""))
        with path.open("rb") as handle:
            response = session.put(relative_url(base_url, relative), data=handle, timeout=120)
            response.raise_for_status()
        uploaded += 1
        logger(f"已上传 {relative}")
    return {"uploaded": uploaded, "skipped": skipped, "total_local": len(local_files)}


def pull_from_webdav(codex_dir: Path, base_url: str, username: str, password: str, verify_tls: bool, logger) -> dict:
    session = create_session(username, password, verify_tls)
    remote_entries = remote_file_map(session, base_url)
    downloaded = 0
    skipped = 0
    for relative, remote in sorted(remote_entries.items()):
        local_path = codex_dir / relative
        local_exists = local_path.exists()
        local_mtime = local_path.stat().st_mtime if local_exists else None
        if local_exists and remote.last_modified and local_mtime and local_mtime >= remote.last_modified:
            skipped += 1
            continue
        if local_exists:
            backup_local_file(codex_dir, local_path, "pull-overwrite")
        response = session.get(relative_url(base_url, relative), timeout=120)
        response.raise_for_status()
        local_path.parent.mkdir(parents=True, exist_ok=True)
        local_path.write_bytes(response.content)
        downloaded += 1
        logger(f"已下载 {relative}")
    return {"downloaded": downloaded, "skipped": skipped, "total_remote": len(remote_entries)}


class CodexSyncApp:
    def __init__(self, root: tk.Tk) -> None:
        self.root = root
        self.root.title("Codex Sync")
        self.root.geometry("1120x700")
        self.root.minsize(1040, 660)
        self.root.configure(bg=BG)
        self.busy = False
        self.status_text_var = tk.StringVar(value="就绪")
        self.provider_var = tk.StringVar(value="--")
        self.session_count_var = tk.StringVar(value="0")
        self.archived_count_var = tk.StringVar(value="0")
        self.codex_dir_display_var = tk.StringVar(value=str(CODEX_DIR))
        self.pill_buttons: list[dict] = []
        self._configure_styles()
        self._build_ui()
        self.load_config()
        self.refresh_summary()

    def _configure_styles(self) -> None:
        style = ttk.Style()
        try:
            style.theme_use("aqua")
        except tk.TclError:
            pass

    def _build_ui(self) -> None:
        shell = tk.Frame(self.root, bg=BG, padx=14, pady=14)
        shell.pack(fill="both", expand=True)
        shell.columnconfigure(0, weight=1)
        shell.rowconfigure(0, weight=1)

        app_canvas, app_panel = self._rounded_card(shell, padx=0, pady=0, radius=22, fill=WINDOW, outline=SHADOW)
        app_canvas.grid(row=0, column=0, sticky="nsew")
        app_panel.columnconfigure(0, weight=1)
        app_panel.rowconfigure(1, weight=1)

        topbar = tk.Frame(app_panel, bg="#F8FAFF", padx=24, pady=14)
        topbar.grid(row=0, column=0, sticky="ew")
        topbar.columnconfigure(0, weight=1)
        topbar.columnconfigure(1, weight=0)

        title_block = tk.Frame(topbar, bg=WINDOW)
        title_block.configure(bg="#F8FAFF")
        title_block.grid(row=0, column=0, sticky="w")
        brand_row = tk.Frame(title_block, bg="#F8FAFF")
        brand_row.pack(anchor="w")
        mark = tk.Canvas(brand_row, width=28, height=28, bg="#F8FAFF", highlightthickness=0, bd=0)
        self._rounded_rect(mark, 1, 1, 27, 27, 8, fill=ACCENT, outline=ACCENT, width=1)
        mark.create_line(8, 19, 20, 8, fill="white", width=2, smooth=True)
        mark.pack(side="left")
        self._text_label(brand_row, "Codex Sync", 17, "bold", TEXT, "#F8FAFF").pack(side="left", padx=(10, 14))
        tk.Frame(brand_row, bg="#D8DDE8", width=1, height=20).pack(side="left", padx=(0, 14))
        self._text_label(brand_row, "线程同步与 Provider 管理", 11, "normal", "#7B8496", "#F8FAFF").pack(side="left")

        top_right = tk.Frame(topbar, bg=WINDOW)
        top_right.configure(bg="#F8FAFF")
        top_right.grid(row=0, column=1, sticky="e", padx=(18, 0))

        metrics = tk.Frame(top_right, bg=WINDOW)
        metrics.configure(bg="#F8FAFF")
        metrics.pack(anchor="e")
        self._compact_metric(metrics, "Provider", self.provider_var, ACCENT).pack(side="left", padx=(0, 14))
        self._compact_metric(metrics, "活跃", self.session_count_var, "#20A35A").pack(side="left", padx=(0, 14))
        self._compact_metric(metrics, "归档", self.archived_count_var, "#8B5CF6").pack(side="left")

        utility_bar = tk.Frame(top_right, bg=WINDOW)
        utility_bar.configure(bg="#F8FAFF")
        utility_bar.pack(anchor="e", pady=(14, 0))
        self.status_badge_canvas, self.status_badge_label = self._status_badge(utility_bar, self.status_text_var.get())
        self.status_badge_canvas.pack(side="left")
        self.refresh_btn = self._pill_button(
            utility_bar,
            "刷新状态",
            self.refresh_summary,
            kind="secondary",
            width=132,
            height=36,
        )
        self.refresh_btn.pack(side="left", padx=(12, 0))

        workspace = tk.Frame(app_panel, bg=WINDOW, padx=28, pady=24)
        workspace.grid(row=1, column=0, sticky="nsew")
        workspace.columnconfigure(0, weight=1)
        workspace.rowconfigure(2, weight=1)

        quick_actions = tk.Frame(workspace, bg=WINDOW)
        quick_actions.grid(row=0, column=0, sticky="ew")
        quick_actions.columnconfigure((0, 1, 2), weight=1)

        self.pull_btn = self._action_card_button(
            quick_actions,
            "拉取远端线程",
            lambda: self.run_async(self.pull),
            accent=ACCENT,
        )
        self.push_btn = self._action_card_button(
            quick_actions,
            "推送本地线程",
            lambda: self.run_async(self.push),
            accent="#20A35A",
        )
        self.unify_btn = self._action_card_button(
            quick_actions,
            "合并 Provider 线程",
            lambda: self.run_async(self.unify),
            accent="#7C3AED",
        )
        self.pull_btn.grid(row=0, column=0, sticky="ew", padx=(0, 10))
        self.push_btn.grid(row=0, column=1, sticky="ew", padx=5)
        self.unify_btn.grid(row=0, column=2, sticky="ew", padx=(10, 0))

        status_row = tk.Frame(workspace, bg=WINDOW)
        status_row.grid(row=1, column=0, sticky="ew", pady=(18, 0))
        status_row.columnconfigure((0, 1, 2), weight=1)
        self._stat_card(status_row, "当前 Provider", self.provider_var, ACCENT).grid(row=0, column=0, sticky="ew", padx=(0, 10))
        self._stat_card(status_row, "活跃会话", self.session_count_var, "#20A35A").grid(row=0, column=1, sticky="ew", padx=5)
        self._stat_card(status_row, "归档会话", self.archived_count_var, "#6E5AEF").grid(row=0, column=2, sticky="ew", padx=(10, 0))

        body = tk.Frame(workspace, bg=WINDOW)
        body.grid(row=2, column=0, sticky="nsew", pady=(22, 0))
        body.columnconfigure(0, weight=11)
        body.columnconfigure(1, weight=7)
        body.rowconfigure(1, weight=1)

        self._text_label(body, "配置与操作", 15, "bold", TEXT, WINDOW).grid(row=0, column=0, sticky="w")
        self._text_label(body, "运行日志", 15, "bold", TEXT, WINDOW).grid(row=0, column=1, sticky="w", padx=(20, 0))

        left = tk.Frame(body, bg=WINDOW)
        left.grid(row=1, column=0, sticky="nsew", pady=(14, 0))
        left.columnconfigure(0, weight=1)

        right = tk.Frame(body, bg=WINDOW)
        right.grid(row=1, column=1, sticky="nsew", pady=(14, 0), padx=(20, 0))
        right.columnconfigure(0, weight=1)
        right.rowconfigure(0, weight=1)

        config_card_canvas, config_card = self._rounded_card(left, pady=20, padx=20, radius=14, fill="#FEFEFF", outline=BORDER)
        config_card_canvas.grid(row=0, column=0, sticky="nsew")
        config_card.columnconfigure(0, weight=1)
        config_card.columnconfigure(1, weight=1)

        self.base_url_var = tk.StringVar()
        self.username_var = tk.StringVar()
        self.password_var = tk.StringVar()
        self.verify_tls_var = tk.BooleanVar(value=True)
        self.codex_dir_var = tk.StringVar(value=str(CODEX_DIR))

        title_row = tk.Frame(config_card, bg=CARD)
        title_row.grid(row=0, column=0, columnspan=2, sticky="ew")
        title_row.columnconfigure(0, weight=1)
        title_row.configure(bg="#FEFEFF")
        self._text_label(title_row, "WebDAV 配置", 13, "bold", TEXT, "#FEFEFF").grid(row=0, column=0, sticky="w")
        self.connected_badge = self._tiny_badge(title_row, "已连接", tint="#EFFAF3", fg="#20A35A")
        self.connected_badge.grid(row=0, column=1, sticky="e")
        self._text_label(
            config_card,
            "连接信息会保存在本机配置中。",
            11,
            "normal",
            MUTED,
            "#FEFEFF",
        ).grid(row=1, column=0, columnspan=2, sticky="w", pady=(8, 20))

        left_fields = [
            ("服务地址", self.base_url_var, False),
            ("用户名", self.username_var, False),
        ]
        right_fields = [
            ("密码", self.password_var, True),
            ("Codex 目录", self.codex_dir_var, False),
        ]

        for idx, (label, var, is_secret) in enumerate(left_fields, start=2):
            self._text_label(config_card, label, 10, "normal", MUTED, "#FEFEFF").grid(row=idx * 2 - 2, column=0, sticky="w", pady=(0, 6))
            entry = ttk.Entry(config_card, textvariable=var)
            if is_secret:
                entry.configure(show="*")
            entry.grid(row=idx * 2 - 1, column=0, sticky="ew", padx=(0, 14), pady=(0, 13), ipady=6)

        for idx, (label, var, is_secret) in enumerate(right_fields, start=2):
            self._text_label(config_card, label, 10, "normal", MUTED, "#FEFEFF").grid(row=idx * 2 - 2, column=1, sticky="w", pady=(0, 6))
            entry = ttk.Entry(config_card, textvariable=var)
            if is_secret:
                entry.configure(show="*")
            entry.grid(row=idx * 2 - 1, column=1, sticky="ew", pady=(0, 13), ipady=6)

        option_row = tk.Frame(config_card, bg="#FEFEFF")
        option_row.grid(row=6, column=0, columnspan=2, sticky="ew", pady=(4, 0))
        option_row.columnconfigure(0, weight=1)
        ttk.Checkbutton(option_row, text="校验证书 TLS", variable=self.verify_tls_var).grid(row=0, column=0, sticky="w")
        self.save_btn = self._pill_button(option_row, "保存配置", self.save_config, kind="secondary", width=148, height=36)
        self.save_btn.grid(row=0, column=1, sticky="e")

        log_card_canvas, log_card = self._rounded_card(right, pady=0, padx=0, radius=14, fill="#FEFEFF", outline=BORDER)
        log_card_canvas.grid(row=0, column=0, sticky="nsew")
        log_card.columnconfigure(0, weight=1)
        log_card.rowconfigure(1, weight=1)

        log_head = tk.Frame(log_card, bg="#FEFEFF", padx=20, pady=18)
        log_head.grid(row=0, column=0, sticky="ew")
        self._text_label(log_head, "最近日志", 13, "bold", TEXT, "#FEFEFF").pack(anchor="w")

        log_surface_canvas, log_surface = self._rounded_card(log_card, pady=0, padx=0, radius=12, fill="#F9FAFD", outline="#EEF0F5")
        log_surface_canvas.grid(row=1, column=0, sticky="nsew", padx=20, pady=(0, 20))
        log_surface.columnconfigure(0, weight=1)
        log_surface.rowconfigure(0, weight=1)

        self.log_widget = tk.Text(
            log_surface,
            wrap="word",
            height=10,
            bd=0,
            padx=18,
            pady=16,
            bg="#FBFBFD",
            fg=TEXT,
            insertbackground=TEXT,
            highlightthickness=0,
            relief="flat",
            font=("SF Mono", 11),
        )
        self.log_widget.grid(row=0, column=0, sticky="nsew")
        scrollbar = ttk.Scrollbar(log_surface, orient="vertical", command=self.log_widget.yview)
        scrollbar.grid(row=0, column=1, sticky="ns", padx=(0, 10), pady=10)
        self.log_widget.configure(yscrollcommand=scrollbar.set)

    def _rounded_card(self, parent, padx: int, pady: int, radius: int = 22, fill: str = CARD, outline: str = BORDER):
        host_bg = parent.cget("bg")
        canvas = tk.Canvas(parent, bg=host_bg, highlightthickness=0, bd=0, relief="flat")
        inner = tk.Frame(canvas, bg=fill, padx=padx, pady=pady)
        shape = self._rounded_rect(canvas, 1, 1, 100, 100, radius, fill=fill, outline=outline, width=1)
        window = canvas.create_window((1, 1), window=inner, anchor="nw")
        canvas.tag_lower(shape)

        def redraw(_event=None) -> None:
            width = max(canvas.winfo_width(), inner.winfo_reqwidth() + 2)
            height = max(canvas.winfo_height(), inner.winfo_reqheight() + 2, 10)
            canvas.configure(height=height)
            canvas.coords(shape, *self._rounded_rect_points(1, 1, width - 2, height - 2, radius))
            canvas.itemconfigure(shape, fill=fill, outline=outline)
            canvas.coords(window, 1, 1)
            canvas.itemconfigure(window, width=max(width - 2, 0))

        inner.bind("<Configure>", redraw)
        canvas.bind("<Configure>", redraw)
        return canvas, inner

    def _nav_item(self, parent, text: str, active: bool = False) -> tk.Canvas:
        width = 146
        height = 42
        canvas = tk.Canvas(parent, width=width, height=height, bg=SIDEBAR, highlightthickness=0, bd=0, relief="flat")
        fill = "#EAF2FF" if active else SIDEBAR
        outline = "#DDE8FF" if active else SIDEBAR
        fg = ACCENT if active else MUTED
        self._rounded_rect(canvas, 1, 1, width - 2, height - 2, 14, fill=fill, outline=outline, width=1)
        canvas.create_text(22, height / 2, text="◉" if active else "◌", fill=fg, font=("SF Pro Text", 10, "bold"), anchor="w")
        canvas.create_text(42, height / 2, text=text, fill=fg, font=("SF Pro Text", 12, "bold" if active else "normal"), anchor="w")
        return canvas

    def _metric_chip(self, parent, title: str, value_var: tk.StringVar, tint: str, accent: str):
        canvas, inner = self._rounded_card(parent, padx=14, pady=10, radius=18, fill=tint, outline=tint)
        inner.columnconfigure(1, weight=1)
        dot = tk.Canvas(inner, width=10, height=10, bg=tint, highlightthickness=0, bd=0)
        dot.create_oval(1, 1, 9, 9, fill=accent, outline=accent)
        dot.grid(row=0, column=0, rowspan=2, sticky="n", padx=(0, 10), pady=(4, 0))
        self._text_label(inner, title, 10, "normal", MUTED, tint).grid(row=0, column=1, sticky="w")
        self._text_label(inner, value_var, 16, "bold", TEXT, tint).grid(row=1, column=1, sticky="w", pady=(2, 0))
        return canvas

    def _compact_metric(self, parent, title: str, value_var: tk.StringVar, accent: str) -> tk.Frame:
        parent_bg = parent.cget("bg")
        metric = tk.Frame(parent, bg=parent_bg)
        dot = tk.Canvas(metric, width=8, height=8, bg=parent_bg, highlightthickness=0, bd=0)
        dot.create_oval(1, 1, 7, 7, fill=accent, outline=accent)
        dot.pack(side="left", padx=(0, 7))
        self._text_label(metric, title, 11, "normal", MUTED, parent_bg).pack(side="left")
        self._text_label(metric, value_var, 12, "bold", TEXT, parent_bg).pack(side="left", padx=(7, 0))
        return metric

    def _stat_card(self, parent, title: str, value_var: tk.StringVar, accent: str):
        canvas, inner = self._rounded_card(parent, padx=18, pady=14, radius=14, fill="#FBFCFF", outline="#E7EAF2")
        inner.columnconfigure(1, weight=1)
        icon = tk.Canvas(inner, width=34, height=34, bg="#FBFCFF", highlightthickness=0, bd=0)
        self._rounded_rect(icon, 2, 2, 32, 32, 10, fill="#F0F5FF", outline="#E1E9FB", width=1)
        icon.create_oval(13, 13, 21, 21, fill=accent, outline=accent)
        icon.grid(row=0, column=0, rowspan=2, sticky="w", padx=(0, 12))
        self._text_label(inner, title, 10, "normal", MUTED, "#FBFCFF").grid(row=0, column=1, sticky="w")
        self._text_label(inner, value_var, 18, "bold", TEXT, "#FBFCFF").grid(row=1, column=1, sticky="w", pady=(3, 0))
        return canvas

    def _tiny_badge(self, parent, text: str, tint: str, fg: str):
        canvas = tk.Canvas(parent, width=78, height=28, bg=parent.cget("bg"), highlightthickness=0, bd=0, relief="flat")
        self._rounded_rect(canvas, 1, 1, 76, 26, 13, fill=tint, outline=tint, width=1)
        canvas.create_text(39, 14, text=text, fill=fg, font=("SF Pro Text", 10, "bold"))
        return canvas

    def _rounded_rect(self, canvas: tk.Canvas, x1: int, y1: int, x2: int, y2: int, r: int, **kwargs):
        return canvas.create_polygon(
            *self._rounded_rect_points(x1, y1, x2, y2, r),
            smooth=True,
            splinesteps=24,
            **kwargs,
        )

    def _rounded_rect_points(self, x1: int, y1: int, x2: int, y2: int, r: int):
        return [
            x1 + r, y1,
            x2 - r, y1,
            x2, y1,
            x2, y1 + r,
            x2, y2 - r,
            x2, y2,
            x2 - r, y2,
            x1 + r, y2,
            x1, y2,
            x1, y2 - r,
            x1, y1 + r,
            x1, y1,
        ]

    def _status_badge(self, parent, text: str):
        canvas = tk.Canvas(parent, width=114, height=36, bg=parent.cget("bg"), highlightthickness=0, bd=0, relief="flat")
        self._rounded_rect(canvas, 1, 1, 112, 34, 18, fill=ACCENT_SOFT, outline="#D8E7FF", width=1)
        label = canvas.create_text(57, 18, text=text, fill=ACCENT, font=("SF Pro Text", 11, "bold"))
        return canvas, label

    def _pill_button(self, parent, text: str, command, kind: str, width: int = 0, height: int = 44):
        frame_bg = parent.cget("bg")
        canvas_width = width or 160
        fill = PILL_FILL if kind == "primary" else PILL_SECONDARY_FILL
        border = PILL_BORDER if kind == "primary" else PILL_SECONDARY_BORDER
        hover = PILL_HOVER if kind == "primary" else "#ECECEF"
        font = ("SF Pro Display", 14, "bold") if kind == "primary" else ("SF Pro Text", 12, "normal")

        canvas = tk.Canvas(
            parent,
            width=canvas_width,
            height=height,
            bg=frame_bg,
            highlightthickness=0,
            bd=0,
            relief="flat",
            cursor="hand2",
        )
        shape = self._rounded_rect(canvas, 1, 1, canvas_width - 2, height - 2, 16, fill=fill, outline=border, width=1)
        label = canvas.create_text(canvas_width / 2, height / 2, text=text, fill=TEXT, font=font)

        spec = {
            "canvas": canvas,
            "shape": shape,
            "label": label,
            "fill": fill,
            "border": border,
            "hover": hover,
            "disabled_fill": "#F3F3F5",
            "disabled_border": "#E3E3E8",
            "disabled_text": "#B6B6BE",
            "text": TEXT,
            "command": command,
        }

        def redraw(current_fill: str, current_border: str, current_text: str) -> None:
            canvas.itemconfigure(shape, fill=current_fill, outline=current_border)
            canvas.itemconfigure(label, fill=current_text)

        def enter(_event) -> None:
            if self.busy:
                return
            redraw(spec["hover"], spec["border"], spec["text"])

        def leave(_event) -> None:
            if self.busy:
                return
            redraw(spec["fill"], spec["border"], spec["text"])

        def click(_event) -> None:
            if self.busy:
                return
            spec["command"]()

        canvas.bind("<Enter>", enter)
        canvas.bind("<Leave>", leave)
        canvas.bind("<Button-1>", click)
        self.pill_buttons.append(spec)
        return canvas

    def _action_card_button(self, parent, title: str, command, accent: str):
        width = 240
        height = 64
        canvas = tk.Canvas(parent, width=width, height=height, bg=parent.cget("bg"), highlightthickness=0, bd=0, relief="flat", cursor="hand2")
        shape = self._rounded_rect(canvas, 1, 1, width - 2, height - 2, 17, fill="#FCFDFF", outline="#DDE3EE", width=1)
        shine = canvas.create_line(18, 8, width - 18, 8, fill="#FFFFFF", width=1)
        icon_bg = self._rounded_rect(canvas, 18, 16, 48, 46, 11, fill="#F1F6FF", outline="#E2EBFA", width=1)
        dot = canvas.create_oval(29, 27, 37, 35, fill=accent, outline=accent)
        title_id = canvas.create_text(62, 32, text=title, fill=TEXT, font=("SF Pro Display", 14, "bold"), anchor="w")
        arrow_id = canvas.create_text(width - 24, 32, text="›", fill=accent, font=("SF Pro Display", 18, "bold"))

        spec = {
            "canvas": canvas,
            "shape": shape,
            "items": (shine, icon_bg, dot, title_id, arrow_id),
            "fill": "#FBFCFF",
            "border": "#DDE2EC",
            "hover": "#F5F8FF",
            "disabled_fill": "#F3F3F5",
            "disabled_border": "#E3E3E8",
            "disabled_text": "#B6B6BE",
            "command": command,
            "accent": accent,
            "title": title_id,
            "arrow": arrow_id,
            "dot": dot,
        }

        def apply_state(fill: str, border: str, title_color: str, accent_color: str) -> None:
            canvas.itemconfigure(shape, fill=fill, outline=border)
            canvas.itemconfigure(title_id, fill=title_color)
            canvas.itemconfigure(arrow_id, fill=accent_color)
            canvas.itemconfigure(dot, fill=accent_color, outline=accent_color)

        def redraw(_event=None) -> None:
            current_width = max(canvas.winfo_width(), width)
            canvas.coords(shape, *self._rounded_rect_points(1, 1, current_width - 2, height - 2, 17))
            canvas.coords(shine, 18, 8, current_width - 18, 8)
            canvas.coords(arrow_id, current_width - 24, 32)

        def enter(_event) -> None:
            if self.busy:
                return
            apply_state(spec["hover"], "#D8E4F8", TEXT, spec["accent"])

        def leave(_event) -> None:
            if self.busy:
                return
            apply_state(spec["fill"], spec["border"], TEXT, spec["accent"])

        def click(_event) -> None:
            if self.busy:
                return
            spec["command"]()

        canvas.bind("<Enter>", enter)
        canvas.bind("<Leave>", leave)
        canvas.bind("<Button-1>", click)
        canvas.bind("<Configure>", redraw)
        self.pill_buttons.append(spec)
        return canvas

    def _text_label(
        self,
        parent,
        text_or_var,
        size: int,
        weight: str,
        fg: str,
        bg: str,
        wraplength: int | None = None,
    ) -> tk.Label:
        kwargs = {
            "fg": fg,
            "bg": bg,
            "font": ("SF Pro Display" if weight == "bold" else "SF Pro Text", size, weight),
            "anchor": "w",
            "justify": "left",
        }
        if wraplength is not None:
            kwargs["wraplength"] = wraplength
        if isinstance(text_or_var, tk.StringVar):
            kwargs["textvariable"] = text_or_var
        else:
            kwargs["text"] = text_or_var
        return tk.Label(parent, **kwargs)

    def log(self, message: str) -> None:
        stamp = datetime.now().strftime("%H:%M:%S")
        self.log_widget.insert("end", f"[{stamp}] {message}\n")
        self.log_widget.see("end")

    def set_status(self, text: str) -> None:
        self.status_text_var.set(text)
        if hasattr(self, "status_badge_canvas") and hasattr(self, "status_badge_label"):
            self.status_badge_canvas.itemconfigure(self.status_badge_label, text=text)

    def codex_dir(self) -> Path:
        return Path(self.codex_dir_var.get()).expanduser()

    def config_payload(self) -> dict:
        base_url = self.base_url_var.get().strip()
        if base_url and not base_url.endswith("/"):
            base_url += "/"
        return {
            "base_url": base_url,
            "username": self.username_var.get().strip(),
            "password": self.password_var.get(),
            "verify_tls": bool(self.verify_tls_var.get()),
        }

    def load_config(self) -> None:
        if CONFIG_PATH.exists():
            payload = json.loads(CONFIG_PATH.read_text(encoding="utf-8"))
            self.base_url_var.set(payload.get("base_url", ""))
            self.username_var.set(payload.get("username", ""))
            self.password_var.set(payload.get("password", ""))
            self.verify_tls_var.set(bool(payload.get("verify_tls", True)))
            self.log(f"已加载配置：{CONFIG_PATH}")

    def save_config(self) -> None:
        payload = self.config_payload()
        CONFIG_PATH.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        self.log(f"已保存配置：{CONFIG_PATH}")
        self.set_status("配置已保存")
        messagebox.showinfo("保存成功", f"配置已保存到：\n{CONFIG_PATH}")

    def refresh_summary(self) -> None:
        codex_dir = self.codex_dir()
        sessions = len(list((codex_dir / "sessions").rglob("rollout-*.jsonl"))) if (codex_dir / "sessions").exists() else 0
        archived = (
            len(list((codex_dir / "archived_sessions").rglob("rollout-*.jsonl")))
            if (codex_dir / "archived_sessions").exists()
            else 0
        )
        provider = "--"
        config_toml = codex_dir / "config.toml"
        if config_toml.exists():
            try:
                provider = read_provider(config_toml)
            except Exception:
                provider = "--"
        self.provider_var.set(provider)
        self.session_count_var.set(str(sessions))
        self.archived_count_var.set(str(archived))
        self.codex_dir_display_var.set(str(codex_dir))
        if not self.busy:
            self.set_status("就绪")

    def set_busy(self, busy: bool) -> None:
        self.busy = busy
        for spec in self.pill_buttons:
            canvas = spec["canvas"]
            if busy:
                canvas.configure(cursor="watch")
                canvas.itemconfigure(spec["shape"], fill=spec["disabled_fill"], outline=spec["disabled_border"])
                if "label" in spec:
                    canvas.itemconfigure(spec["label"], fill=spec["disabled_text"])
                else:
                    canvas.itemconfigure(spec["title"], fill=spec["disabled_text"])
                    canvas.itemconfigure(spec["arrow"], fill=spec["disabled_text"])
                    canvas.itemconfigure(spec["dot"], fill=spec["disabled_text"], outline=spec["disabled_text"])
            else:
                canvas.configure(cursor="hand2")
                canvas.itemconfigure(spec["shape"], fill=spec["fill"], outline=spec["border"])
                if "label" in spec:
                    canvas.itemconfigure(spec["label"], fill=spec["text"])
                else:
                    canvas.itemconfigure(spec["title"], fill=TEXT)
                    canvas.itemconfigure(spec["arrow"], fill=spec["accent"])
                    canvas.itemconfigure(spec["dot"], fill=spec["accent"], outline=spec["accent"])

    def run_async(self, func) -> None:
        if self.busy:
            return

        def worker() -> None:
            self.root.after(0, lambda: self.set_busy(True))
            try:
                func()
            except Exception as exc:
                self.root.after(0, lambda: self.log(f"发生错误：{exc}"))
                self.root.after(0, lambda: self.set_status("操作失败"))
                self.root.after(0, lambda: messagebox.showerror("发生错误", str(exc)))
            finally:
                self.root.after(0, self.refresh_summary)
                self.root.after(0, lambda: self.set_busy(False))

        threading.Thread(target=worker, daemon=True).start()

    def pull(self) -> None:
        payload = self.config_payload()
        self.root.after(0, lambda: self.set_status("正在拉取"))
        self.root.after(0, lambda: self.log("开始从 WebDAV 拉取远端线程"))
        result = pull_from_webdav(
            self.codex_dir(),
            payload["base_url"],
            payload["username"],
            payload["password"],
            payload["verify_tls"],
            lambda msg: self.root.after(0, lambda m=msg: self.log(m)),
        )
        self.root.after(0, lambda: self.set_status("拉取完成"))
        self.root.after(0, lambda: self.log(f"拉取完成：{result}"))

    def push(self) -> None:
        payload = self.config_payload()
        self.root.after(0, lambda: self.set_status("正在推送"))
        self.root.after(0, lambda: self.log("开始推送本地线程到 WebDAV"))
        result = push_to_webdav(
            self.codex_dir(),
            payload["base_url"],
            payload["username"],
            payload["password"],
            payload["verify_tls"],
            lambda msg: self.root.after(0, lambda m=msg: self.log(m)),
        )
        self.root.after(0, lambda: self.set_status("推送完成"))
        self.root.after(0, lambda: self.log(f"推送完成：{result}"))

    def unify(self) -> None:
        self.root.after(0, lambda: self.set_status("正在合并"))
        self.root.after(0, lambda: self.log("开始合并本地所有 Provider 线程"))
        result = unify_provider(self.codex_dir(), lambda msg: self.root.after(0, lambda m=msg: self.log(m)))
        self.root.after(0, lambda: self.set_status("合并完成"))
        self.root.after(0, lambda: self.log(f"合并完成：{result}"))


def main() -> None:
    root = tk.Tk()
    CodexSyncApp(root)
    root.mainloop()


if __name__ == "__main__":
    main()
