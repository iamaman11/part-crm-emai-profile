"""A browser page crash is observed without managing Firefox or leaking page data."""
import contextlib
import io
import unittest
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from native_process import NativeProfileSession


class FakeEmitter:
    def __init__(self):
        self.events = {}

    def on(self, name, fn):
        self.events.setdefault(name, []).append(fn)

    def fire(self, name, *args):
        for fn in self.events.get(name, []):
            fn(*args)


class FakeContext(FakeEmitter):
    def __init__(self, pages):
        super().__init__()
        self.pages = pages
        self.browser = None

    def is_closed(self):
        return False


class PageCrashDiagnosticTests(unittest.TestCase):
    def test_crash_on_existing_and_future_page_is_only_a_fixed_token(self):
        existing, future = FakeEmitter(), FakeEmitter()
        context = FakeContext([existing])
        session = NativeProfileSession(manager=object(), context=context)
        context.fire("page", future)
        output = io.StringIO()
        with contextlib.redirect_stderr(output):
            existing.fire("crash")
            future.fire("crash")
        self.assertEqual(output.getvalue().splitlines(), [
            "camouhost_page_event kind=crash",
            "camouhost_page_event kind=crash",
        ])
        self.assertFalse(session._closed.is_set())
        self.assertFalse(session.context_is_closed())


if __name__ == "__main__":
    unittest.main()
