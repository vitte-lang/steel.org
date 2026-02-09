from datetime import date, timedelta
from typing import List, Tuple

from .model import Book, Catalog


def status_line(book: Book) -> str:
    state = "OUT" if book.checked_out else "IN"
    return f"[{state}] {book.title} — {book.author} ({book.year}) | {book.isbn}"


def list_rows(books: List[Book]) -> List[Tuple[str, str, str, str, str]]:
    rows = []
    for b in books:
        rows.append((
            "OUT" if b.checked_out else "IN",
            b.title,
            b.author,
            str(b.year),
            b.isbn,
        ))
    return rows


def checkout_default(catalog: Catalog, isbn: str, user: str) -> bool:
    due = date.today() + timedelta(days=14)
    return catalog.checkout(isbn, user, due)
