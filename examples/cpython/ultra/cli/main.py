import os
import sys

from app.state import AppState
from core.data import seed_catalog
from core.logic import status_line


def main() -> int:
    if os.environ.get("CPYTHON_ULTRA_GUI", "0") == "1":
        from cli.gui import main as gui_main
        return gui_main()

    state = AppState(catalog=seed_catalog())
    for b in state.catalog.books:
        print(status_line(b))
    print("\nHint: run with CPYTHON_ULTRA_GUI=1 for the GUI")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
