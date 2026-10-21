"""The Qt Widgets reference window (change 0001): the widgets of gtk3.py in
Qt, for recording trees and checking clicks. Prints "clicked NAME" when a
button is clicked. Qt exposes its tree only with
QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1 or a screen reader running.

    QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1 python3 -I spec/reference/qt.py [APP_ID]
"""

import sys

from PySide6.QtGui import QGuiApplication
from PySide6.QtWidgets import (QApplication, QCheckBox, QComboBox, QLabel, QLineEdit, QPushButton,
                               QVBoxLayout, QWidget)

app = QApplication(sys.argv[:1])
QGuiApplication.setDesktopFileName(sys.argv[1] if len(sys.argv) > 1 else "thodu-reference-qt")
window = QWidget()
window.setWindowTitle("Thodu Qt reference")
window.resize(480, 360)
layout = QVBoxLayout(window)
for name in ("Open", "Save", "Close"):
    button = QPushButton(name)
    button.clicked.connect(lambda _c=False, n=name: print(f"clicked {n}", flush=True))
    layout.addWidget(button)
hidden = QPushButton("Hidden")
hidden.hide()
layout.addWidget(hidden)
entry = QLineEdit()
entry.setPlaceholderText("Search")
entry.textChanged.connect(lambda t: print(f"typed {t}", flush=True))
layout.addWidget(entry)
layout.addWidget(QCheckBox("Remember"))
combo = QComboBox()
combo.addItems(["Small", "Medium", "Large"])
layout.addWidget(combo)
link = QLabel('<a href="https://example.org">Docs</a>')
link.setOpenExternalLinks(False)
layout.addWidget(link)
layout.addStretch()
window.show()
sys.exit(app.exec())
