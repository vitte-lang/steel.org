from .model import Book, Catalog


def seed_catalog() -> Catalog:
    catalog = Catalog()
    catalog.add_book(Book(isbn="978-0143127741", title="The Martian", author="Andy Weir", year=2014, tags=["sci-fi", "space"]))
    catalog.add_book(Book(isbn="978-0261103573", title="The Hobbit", author="J.R.R. Tolkien", year=1937, tags=["fantasy"]))
    catalog.add_book(Book(isbn="978-0201633610", title="Design Patterns", author="GoF", year=1994, tags=["software", "architecture"]))
    catalog.add_book(Book(isbn="978-0132350884", title="Clean Code", author="Robert C. Martin", year=2008, tags=["software", "craft"] ))
    catalog.add_book(Book(isbn="978-0131103627", title="The C Programming Language", author="K&R", year=1988, tags=["systems", "classic"]))
    return catalog
