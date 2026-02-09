from dataclasses import dataclass, field
from typing import Optional

from core.model import Catalog, Book


@dataclass
class AppState:
    catalog: Catalog
    selected_isbn: Optional[str] = None
    last_message: str = ""
    filter_text: str = ""
    selected_book: Optional[Book] = None
    selected_index: Optional[int] = None
    history: list[str] = field(default_factory=list)

    def set_message(self, msg: str) -> None:
        self.last_message = msg
        self.history.append(msg)
