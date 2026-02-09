import tkinter as tk
from tkinter import ttk
from datetime import date

from app.controller import add_book, checkin, checkout, refresh_view, select_book
from app.state import AppState
from core.data import seed_catalog
from coreic import list_rows
from core.model import Book


class CatalogApp(tk.Tk):
    def __init__(self) -> None:
        super().__init__()
        self.title("Library Catalog Simulator")
        self.geometry("980x640")
        self.minsize(900, 560)

        self.state = AppState(catalog=seed_catalog())

        self._build_layout()
        self._refresh_table()

    def _build_layout(self) -> None:
        header = ttk.Frame(self)
        header.pack(fill=tk.X, padx=16, pady=12)

        title = ttk.Label(header, text="Library Catalog Simulator", font=("Helvetica", 18, "bold"))
        title.pack(side=tk.LEFT)

        self.message = ttk.Label(header, text="", foreground="#444")
        self.message.pack(side=tk.RIGHT)

        content = ttk.Frame(self)
        content.pack(fill=tk.BOTH, expand=True, padx=16, pady=8)

        left = ttk.Frame(content)
        left.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)

        right = ttk.Frame(content)
        right.pack(side=tk.RIGHT, fill=tk.Y)

        filter_row = ttk.Frame(left)
        filter_row.pack(fill=tk.X, pady=(0, 8))

        ttk.Label(filter_row, text="Filter:").pack(side=tk.LEFT)
        self.filter_var = tk.StringVar()
        filter_entry = ttk.Entry(filter_row, textvariable=self.filter_var)
        filter_entry.pack(side=tk.LEFT, fill=tk.X, expand=True, padx=8)
        filter_entry.bind("<KeyRelease>", lambda _e: self._on_filter())

        self.table = ttk.Treeview(left, columns=("status", "title", "author", "year", "isbn"), show="headings")
        for col, label in (
            ("status", "Status"),
            ("title", "Title"),
            ("author", "Author"),
            ("year", "Year"),
            ("isbn", "ISBN"),
        ):
            self.table.heading(col, text=label)
            self.table.column(col, width=120, anchor=tk.W)
        self.table.column("title", width=280)
        self.table.column("author", width=200)
        self.table.column("isbn", width=160)
        self.table.pack(fill=tk.BOTH, expand=True)
        self.table.bind("<<TreeviewSelect>>", lambda _e: self._on_select())

        controls = ttk.LabelFrame(right, text="Actions")
        controls.pack(fill=tk.X, pady=(0, 12))

        self.user_var = tk.StringVar(value="guest")
        ttk.Label(controls, text="User").pack(anchor=tk.W, padx=8, pady=(8, 0))
        ttk.Entry(controls, textvariable=self.user_var).pack(fill=tk.X, padx=8, pady=(0, 8))

        ttk.Button(controls, text="Checkout", command=self._checkout).pack(fill=tk.X, padx=8, pady=4)
        ttk.Button(controls, text="Checkin", command=self._checkin).pack(fill=tk.X, padx=8, pady=4)

        add_box = ttk.LabelFrame(right, text="Add Book")
        add_box.pack(fill=tk.X)

        self.add_title = tk.StringVar()
        self.add_author = tk.StringVar()
        self.add_year = tk.StringVar(value=str(date.today().year))
        self.add_isbn = tk.StringVar()
        self.add_tags = tk.StringVar()

        for label, var in (
            ("Title", self.add_title),
            ("Author", self.add_author),
            ("Year", self.add_year),
            ("ISBN", self.add_isbn),
            ("Tags", self.add_tags),
        ):
            ttk.Label(add_box, text=label).pack(anchor=tk.W, padx=8, pady=(8, 0))
            ttk.Entry(add_box, textvariable=var).pack(fill=tk.X, padx=8, pady=(0, 4))

        ttk.Button(add_box, text="Add to Catalog", command=self._add_book).pack(fill=tk.X, padx=8, pady=8)

        history = ttk.LabelFrame(right, text="History")
        history.pack(fill=tk.BOTH, expand=True, pady=(12, 0))
        self.history_box = tk.Text(history, height=12, state=tk.DISABLED, wrap=tk.WORD)
        self.history_box.pack(fill=tk.BOTH, expand=True, padx=8, pady=8)

    def _on_filter(self) -> None:
        self.state.filter_text = self.filter_var.get()
        self._refresh_table()

    def _on_select(self) -> None:
        item = self.table.selection()
        if not item:
            return
        values = self.table.item(item[0], "values")
        if not values:
            return
        isbn = values[4]
        for b in self.state.catalog.books:
            if b.isbn == isbn:
                select_book(self.state, b)
                break

    def _checkout(self) -> None:
        ok = checkout(self.state, self.user_var.get().strip() or "guest")
        self._after_action(ok)

    def _checkin(self) -> None:
        ok = checkin(self.state)
        self._after_action(ok)

    def _add_book(self) -> None:
        try:
            year = int(self.add_year.get())
        except ValueError:
            self.state.set_message("Year must be a number")
            self._refresh_message()
            return
        book = Book(
            isbn=self.add_isbn.get().strip(),
            title=self.add_title.get().strip(),
            author=self.add_author.get().strip(),
            year=year,
            tags=[t.strip() for t in self.add_tags.get().split(",") if t.strip()],
        )
        add_book(self.state, book)
        self._refresh_table()
        self._refresh_message()

    def _after_action(self, _ok: bool) -> None:
        self._refresh_table()
        self._refresh_message()

    def _refresh_message(self) -> None:
        self.message.config(text=self.state.last_message)
        self.history_box.config(state=tk.NORMAL)
        self.history_box.insert(tk.END, self.state.last_message + "\n")
        self.history_box.config(state=tk.DISABLED)
        self.history_box.see(tk.END)

    def _refresh_table(self) -> None:
        for row in self.table.get_children():
            self.table.delete(row)
        books = refresh_view(self.state)
        for row in list_rows(books):
            self.table.insert("", tk.END, values=row)


def main() -> int:
    app = CatalogApp()
    app.mainloop()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
