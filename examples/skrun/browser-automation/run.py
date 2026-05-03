#!/usr/bin/env python3
from __future__ import annotations

import sys
from pathlib import Path

sys.path.append(str(Path(__file__).resolve().parents[1] / "_lib"))
from common import fail, ok, read_input, string_input


def main() -> None:
    payload = read_input()
    url = string_input(payload, "url")
    screenshot_path = payload.get("screenshot_path")
    try:
        from playwright.sync_api import sync_playwright
    except ImportError:
        fail("missing Python package: playwright. Install it in the skrun skill environment.")

    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page()
        page.goto(url, wait_until="networkidle")
        title = page.title()
        shot = None
        if isinstance(screenshot_path, str) and screenshot_path:
            page.screenshot(path=screenshot_path, full_page=True)
            shot = screenshot_path
        browser.close()
    ok(url=url, title=title, screenshot_path=shot)


if __name__ == "__main__":
    main()
