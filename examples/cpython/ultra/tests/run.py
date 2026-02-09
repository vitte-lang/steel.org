from app.state import AppState
from core.data import seed_catalog
from core.logic import checkout_default


def test_checkout_cycle() -> None:
    state = AppState(catalog=seed_catalog())
    book = state.catalog.books[0]
    ok = checkout_default(state.catalog, book.isbn, "tester")
    assert ok
    assert book.checked_out
    ok = state.catalog.checkin(book.isbn)
    assert ok
    assert not book.checked_out


def test_find() -> None:
    state = AppState(catalog=seed_catalog())
    books = state.catalog.find("tolkien")
    assert books


def main() -> int:
    test_checkout_cycle()
    test_find()
    print("tests ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
