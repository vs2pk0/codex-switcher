"""Offline native regression for an explicitly selected local chat copy.

python3 tests/nativeSessionReplay.py --source-home ~/.codex --thread-id UUID \
  --codex-bin /path/to/codex [--repair-mode context] [--revert-turn-id UUID]

Requires Python 3, Cargo, and zstd (for native OAuth request compression).
Only JSONL files and history projections are copied. Credentials are synthetic;
both OAuth and API providers point to a localhost SSE server. The live home is
read only. Temporary copies/captured requests stay local for inspection.
"""
import argparse
import base64
import collections
import http.server
import json
import os
from pathlib import Path
import queue
import shutil
import sqlite3
import subprocess
import tempfile
import threading
import time


def first_record(path):
    with path.open() as stream:
        return json.loads(next(line for line in stream if line.strip()))


def copy_lineage(source, fixture, thread):
    candidates = []
    physical = {}
    for directory in ("sessions", "archived_sessions"):
        for path in (source / directory).rglob("*.jsonl"):
            record = first_record(path)
            logical = record["payload"]["id"]
            identifier = path.stem.rsplit("_", 1)[-1] if "_" in path.stem else logical
            current = physical.get(identifier)
            if current is None or path.stat().st_size > current.stat().st_size:
                physical[identifier] = path
            if logical == thread:
                candidates.append(path)
    assert candidates, "No matching local chat"
    head = max(candidates, key=lambda path: path.stat().st_mtime_ns)
    for state in sorted(source.glob("state_*.sqlite"), reverse=True):
        with sqlite3.connect(f"file:{state}?mode=ro", uri=True) as connection:
            row = connection.execute("SELECT rollout_path FROM threads WHERE id=?", (thread,)).fetchone()
            if row and Path(row[0]).is_file():
                head = Path(row[0])
                break
    current = head
    identifiers = set()
    while current:
        record = first_record(current)
        logical = record["payload"]["id"]
        identifier = current.stem.rsplit("_", 1)[-1] if "_" in current.stem else logical
        assert identifier not in identifiers, "Cyclic lineage"
        identifiers.add(identifier)
        target = fixture / current.relative_to(source)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(current, target)
        base = record["payload"].get("history_base") or record.get("history_base")
        current = physical[base["thread_id"]] if base else None
    for path in source.glob("thread_history_*.sqlite"):
        with sqlite3.connect(f"file:{path}?mode=ro", uri=True) as original:
            with sqlite3.connect(fixture / path.name) as copied:
                original.backup(copied)
                for (table,) in copied.execute("SELECT name FROM sqlite_master WHERE type='table'").fetchall():
                    quoted = '"' + table.replace('"', '""') + '"'
                    columns = {row[1] for row in copied.execute(f"PRAGMA table_info({quoted})")}
                    if "thread_id" in columns:
                        placeholders = ",".join("?" for _ in identifiers)
                        copied.execute(f"DELETE FROM {quoted} WHERE thread_id NOT IN ({placeholders})", tuple(identifiers))
    return fixture / head.relative_to(source)


class Runtime:
    def __init__(self, binary, fixture):
        self.log = (fixture / "runtime-stderr.log").open("a")
        self.process = subprocess.Popen(
            [binary, "app-server", "--stdio"],
            env=dict(os.environ, CODEX_HOME=str(fixture)),
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.log,
            text=True, bufsize=1,
        )
        self.messages = queue.Queue()
        self.counter = 0
        threading.Thread(target=self.read, daemon=True).start()

    def read(self):
        for line in self.process.stdout:
            self.messages.put(json.loads(line))

    def wait(self, predicate):
        deadline = time.monotonic() + 45
        while time.monotonic() < deadline:
            message = self.messages.get(timeout=max(.01, deadline - time.monotonic()))
            if predicate(message):
                return message
        raise TimeoutError("Native runtime did not finish")

    def rpc(self, method, params):
        self.counter += 1
        identifier = self.counter
        self.process.stdin.write(json.dumps(dict(id=identifier, method=method, params=params)) + "\n")
        self.process.stdin.flush()
        response = self.wait(lambda message: message.get("id") == identifier)
        assert "error" not in response, (method, response.get("error"))
        return response["result"]

    def turn(self, thread, model, text):
        turn = self.rpc("turn/start", dict(
            threadId=thread, model=model, input=[dict(type="text", text=text)],
            approvalPolicy="never", sandboxPolicy=dict(type="readOnly"),
        ))["turn"]
        result = self.wait(lambda message: message.get("method") == "turn/completed"
                           and message["params"]["turn"]["id"] == turn["id"])
        assert result["params"]["turn"]["status"] == "completed", result
        return turn["id"]

    def close(self):
        self.process.terminate()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()
        self.log.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-home", required=True, type=Path)
    parser.add_argument("--thread-id", required=True)
    parser.add_argument("--codex-bin", required=True)
    parser.add_argument("--repair-mode", choices=("switch", "context"), default="switch")
    parser.add_argument("--revert-turn-id")
    parser.add_argument("--model-catalog", type=Path, help="Optional provider model catalog to copy into the fixture")
    options = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    fixture = Path(tempfile.mkdtemp(prefix="codex-switcher-session-check-", dir="/tmp"))
    print("FIXTURE", fixture, flush=True)
    head = copy_lineage(options.source_home.expanduser().resolve(), fixture, options.thread_id)
    if options.model_catalog:
        shutil.copy2(options.model_catalog, fixture / "api-models.json")
    payload = dict(exp=int(time.time()) + 31536000, email="fixture@example.invalid")
    payload["https://api.openai.com/auth"] = dict(chatgpt_account_id="fixture-account", chatgpt_plan_type="plus", chatgpt_user_id="fixture-user")
    token = "eyJhbGciOiJub25lIn0." + base64.urlsafe_b64encode(json.dumps(payload).encode()).decode().rstrip("=") + ".fixture"
    (fixture / "auth.json").write_text(json.dumps(dict(tokens=dict(
        id_token=token, access_token=token, refresh_token="fixture", account_id="fixture-account"))))
    requests = queue.Queue()

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            body = b'{"models":[],"data":[]}'
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_POST(self):
            raw = self.rfile.read(int(self.headers["Content-Length"]))
            if self.headers.get("Content-Encoding") == "zstd":
                raw = subprocess.run(["zstd", "-dq", "-c"], input=raw, capture_output=True, check=True).stdout
            body = json.loads(raw)
            if not self.path.endswith("/responses"):
                self.send_error(404)
                return
            requests.put(body)
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.end_headers()
            number = str(time.time_ns())
            item = dict(type="message", id="msg_" + number, role="assistant", status="completed",
                        content=[dict(type="output_text", text="LOCAL_OK", annotations=[])])
            response = dict(id="resp_" + number, object="response", status="completed", output=[item],
                            usage=dict(input_tokens=100, output_tokens=5, total_tokens=105))
            for event in [
                dict(type="response.created", response=dict(response, status="in_progress", output=[])),
                dict(type="response.output_item.added", output_index=0, item=dict(item, content=[])),
                dict(type="response.output_text.delta", output_index=0, content_index=0, item_id=item["id"], delta="LOCAL_OK"),
                dict(type="response.output_item.done", output_index=0, item=item),
                dict(type="response.completed", response=response),
            ]:
                self.wfile.write(("event: " + event["type"] + "\ndata: " + json.dumps(event) + "\n\n").encode())
            self.wfile.flush()

        def log_message(self, *args):
            pass

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    url = f"http://127.0.0.1:{server.server_port}"
    previous_turn = options.revert_turn_id
    try:
        for leg, (provider, model) in enumerate([
            ("local_fixture", "DeepSeek-V4.1-Flash"), ("openai", "gpt-6.1-sol"),
            ("local_fixture", "DeepSeek-V4.1-Flash"), ("openai", "gpt-6.1-sol"),
        ]):
            (fixture / "config.toml").write_text(
                (f'model_catalog_json = "{fixture}/api-models.json"\n' if options.model_catalog and provider != "openai" else "") +
                f'openai_base_url = "{url}/v1"\nchatgpt_base_url = "{url}"\n'
                f'model = "{model}"\nmodel_provider = "{provider}"\nweb_search = "disabled"\n'
                f'[model_providers.local_fixture]\nname = "Local fixture"\nbase_url = "{url}/v1"\n'
                'wire_api = "responses"\nrequires_openai_auth = true\nsupports_websockets = false\n'
                'request_max_retries = 0\nstream_max_retries = 0\n'
                '[features]\napps = false\nplugins = false\nresponses_websockets = false\nresponses_websockets_v2 = false\n'
            )
            env = dict(os.environ, CODEX_SWITCHER_REPAIR_FIXTURE_HOME=str(fixture))
            if options.repair_mode == "context":
                env["CODEX_SWITCHER_REPAIR_FIXTURE_CONTEXT"] = "1"
            repair = subprocess.run([
                "cargo", "test", "--lib", "session::portable_history::tests::repair_isolated_real_session_fixture",
                "--offline", "--", "--ignored", "--nocapture",
            ], cwd=repo / "src-tauri", env=env, capture_output=True, text=True)
            assert repair.returncode == 0, repair.stdout + repair.stderr
            runtime = Runtime(options.codex_bin, fixture)
            try:
                runtime.rpc("initialize", dict(clientInfo=dict(name="Codex Desktop", version="fixture"), capabilities=dict(experimentalApi=True)))
                resumed = runtime.rpc("thread/resume", dict(
                    threadId=options.thread_id, path=str(head), excludeTurns=True,
                    deferGoalContinuation=True, cwd=str(fixture),
                    config={"features.incremental_tools": True, "features.deferred_tool_world_state": True},
                ))
                assert resumed["model"].split("/")[-1] == model, resumed["model"]
                if previous_turn:
                    runtime.rpc("thread/revert", dict(threadId=options.thread_id, beforeTurnId=previous_turn))
                for turn in range(2):
                    probe = f"SWITCHER_LOCAL_PROBE_{leg}_{turn}"
                    previous_turn = runtime.turn(options.thread_id, model, probe)
                    body = requests.get(timeout=5)
                    assert body["model"].split("/")[-1] == model
                    assert probe in json.dumps(body)
                    types = collections.Counter(item.get("type") for item in body.get("input", []))
                    if provider != "openai":
                        assert set(types) <= {"message", "function_call", "function_call_output"}, types
                    capture = fixture / f"request-{leg}-{turn}.json"
                    capture.write_text(json.dumps(body))
                    print(json.dumps(dict(leg=leg, turn=turn, provider=provider, model=model,
                                          status="completed", input_types=types)), flush=True)
                # Later legs must resolve the newest physical segment, not an old head.
                result = runtime.rpc("thread/read", dict(threadId=options.thread_id, includeTurns=False))
                head = Path(result["thread"].get("path") or head)
            finally:
                runtime.close()
    finally:
        server.shutdown()
    print("VERIFIED_FIXTURE", fixture, flush=True)


if __name__ == "__main__":
    main()
