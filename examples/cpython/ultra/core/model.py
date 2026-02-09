from dataclasses import dataclass, field
from datetime import date
from typing import List


@dataclass
class Book:
    isbn: str
    title: str
    author: str
    year: int
    tags: List[str] = field(default_factory=list)
    checked_out: bool = False


@dataclass
class Loan:
    isbn: str
    user: str
    due: date


@dataclass
class Catalog:
    books: List[Book] = field(default_factory=list)
    loans: List[Loan] = field(default_factory=list)

    def add_book(self, book: Book) -> None:
        self.books.append(book)

    def find(self, query: str) -> List[Book]:
        q = query.strip().lower()
        if not q:
            return list(self.books)
        out = []
        for b in self.books:
            if (
                q in b.title.lower()
                or q in b.author.lower()
                or q in b.isbn.lower()
                or any(q in t.lower() for t in b.tags)
            ):
                out.append(b)
        return out

    def checkout(self, isbn: str, user: str, due: date) -> bool:
        for b in self.books:
            if b.isbn == isbn and not b.checked_out:
                b.checked_out = True
                self.loans.append(Loan(isbn=isbn, user=user, due=due))
                return True
        return False

    def checkin(self, isbn: str) -> bool:
        for b in self.books:
            if b.isbn == isbn and b.checked_out:
                b.checked_out = False
                self.loans = [l for l in self.loans if l.isbn != isbn]
                return True
        return False
