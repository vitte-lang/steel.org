from typing import List

from coreic import checkout_default
from core.model import Book, Catalog

from .state import AppState


def refresh_view(state: AppState) -> List[Book]:
    state.selected_isbn = None
    state.selected_book = None
    return state.catalog.find(state.filter_text)


def select_book(state: AppState, book: Book) -> None:
    state.selected_isbn = book.isbn
    state.selected_book = book


def checkout(state: AppState, user: str) -> bool:
    if not state.selected_isbn:
        state.set_message("Select a book first")
        return False
    ok = checkout_default(state.catalog, state.selected_isbn, user)
    if ok:
        state.set_message(f"Checked out to {user}")
    else:
        state.set_message("Checkout failed")
    return ok


def checkin(state: AppState) -> bool:
    if not state.selected_isbn:
        state.set_message("Select a book first")
        return False
    ok = state.catalog.checkin(state.selected_isbn)
    if ok:
        state.set_message("Checked in")
    else:
        state.set_message("Checkin failed")
    return ok


def add_book(state: AppState, book: Book) -> None:
    state.catalog.add_book(book)
    state.set_message("Book added")
