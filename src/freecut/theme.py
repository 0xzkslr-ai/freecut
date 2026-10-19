"""Shared desktop UI colors and keyboard focus styling."""

BACKGROUND = "#f4f7f8"
SURFACE = "#ffffff"
TEXT = "#172b35"
MUTED = "#526572"
PRIMARY = "#087f73"
BORDER = "#d7e1e5"

STYLESHEET = f"""
QWidget {{
    color: {TEXT};
    font-family: 'Segoe UI', 'Microsoft YaHei UI', 'Noto Sans CJK SC', sans-serif;
    font-size: 14px;
}}
QMainWindow, QWidget#root {{ background: {BACKGROUND}; }}
QFrame#sidebar, QFrame#previewCard {{
    background: {SURFACE}; border: 1px solid {BORDER}; border-radius: 12px;
}}
QLabel {{ background: transparent; }}
QLabel#brand {{ font-size: 26px; font-weight: 700; }}
QLabel#heading {{ font-size: 22px; font-weight: 600; }}
QLabel#section {{ font-size: 14px; font-weight: 600; }}
QLabel#muted, QLabel#status {{ color: {MUTED}; }}
QLabel#badge {{ color: {PRIMARY}; background: #e4f4ef; border-radius: 10px; padding: 5px 10px; }}
QPushButton {{
    background: {SURFACE}; border: 1px solid {BORDER}; border-radius: 8px;
    padding: 10px 16px; min-height: 20px;
}}
QPushButton:hover {{ background: #f0f7f5; border-color: #78afa6; }}
QPushButton:pressed {{ background: #d8eee8; }}
QPushButton:checked {{ background: #e7f5ef; border: 2px solid {PRIMARY}; }}
QPushButton:focus, QComboBox:focus {{ border: 2px solid {PRIMARY}; }}
QPushButton:disabled {{ color: #74838b; background: #eef2f4; border-color: #e0e6e9; }}
QPushButton#primary {{
    color: white; background: {PRIMARY}; border-color: {PRIMARY}; font-weight: 600;
}}
QPushButton#primary:hover {{ background: #06695f; }}
QPushButton#primary:pressed {{ background: #04564e; }}
QPushButton#primary:disabled {{ color: #f4f7f8; background: #809c98; border-color: #809c98; }}
QPushButton#mode {{ text-align: left; padding: 10px; min-height: 36px; }}
QScrollArea#sidebarScroll {{ border: 0; background: transparent; }}
QComboBox {{
    border: 1px solid {BORDER}; border-radius: 8px; padding: 10px;
    min-height: 20px; background: white;
}}
QComboBox QAbstractItemView {{ background: white; selection-background-color: #d8eee8; }}
QStatusBar {{ background: {BACKGROUND}; color: {MUTED}; }}
QToolTip {{ background: {TEXT}; color: white; border: 0; padding: 6px; }}
"""
