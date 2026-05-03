#!/usr/bin/env python3
from __future__ import annotations

import smtplib
import sys
from email.message import EmailMessage
from pathlib import Path

sys.path.append(str(Path(__file__).resolve().parents[1] / "_lib"))
from common import int_input, ok, read_input, require_env, string_input


def main() -> None:
    payload = read_input()
    to_address = string_input(payload, "to")
    subject = string_input(payload, "subject")
    body = string_input(payload, "body")
    host = require_env("SMTP_HOST")
    port = int_input({"port": require_env("SMTP_PORT")}, "port", default=587, maximum=65535)
    username = require_env("SMTP_USERNAME")
    password = require_env("SMTP_PASSWORD")
    from_address = payload.get("from") or require_env("SMTP_FROM")

    message = EmailMessage()
    message["From"] = from_address
    message["To"] = to_address
    message["Subject"] = subject
    message.set_content(body)

    with smtplib.SMTP(host, port, timeout=30) as smtp:
        smtp.starttls()
        smtp.login(username, password)
        smtp.send_message(message)
    ok(to=to_address, subject=subject)


if __name__ == "__main__":
    main()
