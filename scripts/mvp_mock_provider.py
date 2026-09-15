#!/usr/bin/env python3
"""Deterministic OpenAI-compatible provider used by the MVP acceptance run."""

from __future__ import annotations

import argparse
import json
import re
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


ANALYSIS = {
    "title": "Distributed Consensus",
    "summary": "Raft elects a leader and commits replicated log entries.",
    "entities": [
        {
            "name": "Distributed Consensus",
            "aliases": ["Raft"],
            "summary": "A distributed consensus algorithm that elects a leader.",
        }
    ],
    "concepts": [
        {
            "name": "Leader Election",
            "definition": "Candidates win after a majority grants their term.",
        }
    ],
}

DRAFTS = {
    "source_summary": "Raft elects a leader and commits replicated log entries.",
    "entities": ANALYSIS["entities"],
    "concepts": ANALYSIS["concepts"],
}


def first_evidence(prompt: str) -> dict[str, str] | None:
    pattern = re.compile(
        r"\[1\] path: (?P<path>[^\n]+)\n"
        r"title: (?P<title>[^\n]+)\n"
        r"page_type: (?P<page_type>[^\n]+)\n"
        r"revision: (?P<revision>\{.*?\})\n"
        r"content:\n(?P<content>.*?)(?=\n\n\[\d+\]|\Z)",
        re.DOTALL,
    )
    match = pattern.search(prompt)
    if not match:
        return None
    evidence = match.groupdict()
    lines = (line.strip() for line in evidence["content"].splitlines())
    evidence["quote"] = next((line for line in lines if line), "MVP acceptance evidence")
    return evidence


def completion_content(body: dict) -> str:
    messages = body.get("messages", [])
    def text(value: str | list[dict]) -> str:
        if isinstance(value, str):
            return value
        return "".join(part.get("text", "") for part in value)

    system = next(
        (text(message.get("content", "")) for message in messages if message.get("role") == "system"),
        "",
    )
    prompt = next(
        (text(message.get("content", "")) for message in messages if message.get("role") == "user"),
        "",
    )
    if "precise research analyst" in system:
        return json.dumps(ANALYSIS, ensure_ascii=False)
    if "wiki editor" in system:
        return json.dumps(DRAFTS, ensure_ascii=False)
    if "research assistant" in system:
        evidence = first_evidence(prompt)
        if evidence is None:
            return json.dumps(
                {
                    "answer": "当前 Wiki 证据不足。",
                    "citations": [],
                },
                ensure_ascii=False,
            )
        return json.dumps(
            {
                "answer": "The fixture evidence confirms the requested claim. [1]",
                "citations": [
                    {
                        "number": 1,
                        "path": evidence["path"],
                        "title": evidence["title"],
                        "quote": evidence["quote"],
                        "revision": json.loads(evidence["revision"]),
                    }
                ],
            },
            ensure_ascii=False,
        )
    raise ValueError(f"unknown MVP acceptance request: {json.dumps(body)[:2000]}")


class Handler(BaseHTTPRequestHandler):
    server: "ProviderServer"

    def do_POST(self) -> None:
        if not self.path.endswith("/chat/completions"):
            self.send_error(404, "unknown provider endpoint")
            return
        length = int(self.headers.get("content-length", "0"))
        body = json.loads(self.rfile.read(length) or b"{}")
        content = completion_content(body)
        response = {
            "id": "chatcmpl-mvp-acceptance",
            "object": "chat.completion",
            "created": 0,
            "model": body.get("model", "mvp-mock"),
            "choices": [
                {
                    "index": 0,
                    "message": {"role": "assistant", "content": content},
                    "finish_reason": "stop",
                }
            ],
            "usage": {
                "prompt_tokens": 16,
                "completion_tokens": 32,
                "total_tokens": 48,
            },
        }
        raw = json.dumps(response, ensure_ascii=False).encode()
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(raw)))
        self.send_header("connection", "close")
        self.end_headers()
        self.wfile.write(raw)

    def log_message(self, format: str, *args: object) -> None:
        if self.server.verbose:
            super().log_message(format, *args)


class ProviderServer(ThreadingHTTPServer):
    daemon_threads = True
    verbose = False


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port-file", required=True, type=Path)
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()

    server = ProviderServer(("127.0.0.1", 0), Handler)
    server.verbose = args.verbose
    args.port_file.write_text(str(server.server_port))
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
