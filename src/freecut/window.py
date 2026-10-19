from datetime import datetime
from pathlib import Path

from PySide6.QtCore import QSettings, QStandardPaths, Qt, QTimer
from PySide6.QtGui import QAction, QCursor, QGuiApplication, QImage, QKeySequence
from PySide6.QtWidgets import (
    QApplication,
    QButtonGroup,
    QComboBox,
    QFileDialog,
    QFrame,
    QHBoxLayout,
    QKeySequenceEdit,
    QLabel,
    QLayout,
    QMainWindow,
    QMenu,
    QMessageBox,
    QPushButton,
    QScrollArea,
    QStyle,
    QSystemTrayIcon,
    QVBoxLayout,
    QWidget,
)

from freecut.hotkeys import DEFAULT_HOTKEY, GlobalHotkey, parse_hotkey
from freecut.imaging import image_mime_data, save_png
from freecut.overlay import SelectionOverlay
from freecut.preview import Preview
from freecut.selection import Mode


class MainWindow(QMainWindow):
    def __init__(self, *, settings: QSettings | None = None) -> None:
        super().__init__()
        self.setWindowTitle("FreeCut · 自由形状截图")
        self.resize(1040, 720)
        self.setMinimumSize(820, 600)
        self.mode = Mode.LASSO
        self.image = QImage()
        self._capturing = False
        self._closing = False
        self._return_to_background = False
        self._settings = settings if settings is not None else QSettings("FreeCut", "FreeCut")
        self._hotkey = GlobalHotkey(self)
        self._hotkey.activated.connect(
            self._capture_from_hotkey, Qt.ConnectionType.QueuedConnection
        )
        self._tray: QSystemTrayIcon | None = None
        self._tray_capture_action: QAction | None = None
        self._background_notified = False
        self._overlay: SelectionOverlay | None = None
        self._target_screen = None
        self._capture_timer = QTimer(self)
        self._capture_timer.setSingleShot(True)
        self._capture_timer.timeout.connect(self._take_capture)

        root = QWidget()
        root.setObjectName("root")
        self.setCentralWidget(root)
        layout = QVBoxLayout(root)
        layout.setContentsMargins(28, 24, 28, 16)
        layout.setSpacing(20)
        header = QHBoxLayout()
        brand = QLabel("FreeCut")
        brand.setObjectName("brand")
        header.addWidget(brand)
        tagline = QLabel("让截图，自由一点。")
        tagline.setObjectName("muted")
        header.addWidget(tagline)
        header.addStretch()
        self.background_button = QPushButton("后台运行")
        self.background_button.clicked.connect(self.hide_to_tray)
        header.addWidget(self.background_button)
        badge = QLabel("本地运行 · 无需联网")
        badge.setObjectName("badge")
        header.addWidget(badge)
        layout.addLayout(header)

        body = QHBoxLayout()
        body.setSpacing(20)
        sidebar = QFrame()
        sidebar.setObjectName("sidebar")
        controls = QVBoxLayout(sidebar)
        controls.setSizeConstraint(QLayout.SizeConstraint.SetMinimumSize)
        controls.setContentsMargins(18, 20, 18, 20)
        controls.setSpacing(8)
        controls.addWidget(self._label("选择你的工具", "section"))
        self._mode_group = QButtonGroup(self)
        for mode, description in (
            (Mode.LASSO, "按住鼠标，画出任意轮廓"),
            (Mode.POLYGON, "逐点连线，精确勾勒边界"),
        ):
            button = QPushButton(f"{mode.value}\n{description}")
            button.setObjectName("mode")
            button.setCheckable(True)
            button.setChecked(mode == self.mode)
            button.setCursor(Qt.CursorShape.PointingHandCursor)
            button.setToolTip(description)
            button.clicked.connect(lambda checked=False, value=mode: self._set_mode(value))
            self._mode_group.addButton(button, list(Mode).index(mode))
            controls.addWidget(button)

        controls.addSpacing(8)
        controls.addWidget(self._label("截取屏幕", "section"))
        self.screen_combo = QComboBox()
        self.screen_combo.setAccessibleName("选择要截取的显示器")
        controls.addWidget(self.screen_combo)
        controls.addWidget(self._label("开始前等待", "section"))
        self.delay_combo = QComboBox()
        self.delay_combo.setAccessibleName("截图延迟")
        for name, delay in (("立即截图", 350), ("等待 2 秒", 2000), ("等待 5 秒", 5000)):
            self.delay_combo.addItem(name, delay)
        controls.addWidget(self.delay_combo)
        controls.addSpacing(8)
        self.capture_button = QPushButton("开始截图")
        self.capture_button.setObjectName("primary")
        self.capture_button.setCursor(Qt.CursorShape.PointingHandCursor)
        self.capture_button.setToolTip("Ctrl+Shift+A（应用窗口内）")
        self.capture_button.clicked.connect(self._begin_capture)
        controls.addWidget(self.capture_button)
        controls.addWidget(self._label("Ctrl + Shift + A · 窗口内快捷键", "muted"))
        controls.addWidget(self._label("全局截图快捷键", "section"))
        hotkey_row = QHBoxLayout()
        self.hotkey_edit = QKeySequenceEdit()
        self.hotkey_edit.setAccessibleName("全局截图快捷键")
        self.hotkey_edit.setMaximumSequenceLength(1)
        self.hotkey_edit.setClearButtonEnabled(True)
        self.hotkey_edit.setToolTip("点击后按下快捷键组合，再点击应用。")
        hotkey_row.addWidget(self.hotkey_edit, 1)
        self.hotkey_apply_button = QPushButton("应用")
        self.hotkey_apply_button.clicked.connect(self._apply_hotkey)
        hotkey_row.addWidget(self.hotkey_apply_button)
        controls.addLayout(hotkey_row)
        self.hotkey_status_label = self._label("", "muted")
        self.hotkey_status_label.setWordWrap(True)
        controls.addWidget(self.hotkey_status_label)
        controls.addStretch()
        hint = self._label(
            "Enter 确认 · Esc 取消",
            "muted",
        )
        hint.setWordWrap(True)
        controls.addWidget(hint)
        sidebar_scroll = QScrollArea()
        sidebar_scroll.setObjectName("sidebarScroll")
        sidebar_scroll.setFixedWidth(264)
        sidebar_scroll.setWidgetResizable(True)
        sidebar_scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        sidebar_scroll.setWidget(sidebar)
        body.addWidget(sidebar_scroll)

        card = QFrame()
        card.setObjectName("previewCard")
        content = QVBoxLayout(card)
        content.setContentsMargins(20, 24, 20, 20)
        content.setSpacing(12)
        title = QHBoxLayout()
        title.addWidget(self._label("截图预览", "heading"))
        title.addStretch()
        self.dimensions_label = self._label("尚未截图", "muted")
        title.addWidget(self.dimensions_label)
        content.addLayout(title)
        self.preview = Preview()
        content.addWidget(self.preview, 1)
        content.addWidget(self._label("棋盘格表示透明区域，不会出现在导出的图片中。", "muted"))
        actions = QHBoxLayout()
        actions.addStretch()
        self.copy_button = QPushButton("复制图片  Ctrl+C")
        self.copy_button.clicked.connect(self._copy)
        self.save_button = QPushButton("保存 PNG  Ctrl+S")
        self.save_button.setObjectName("primary")
        self.save_button.clicked.connect(self._save)
        for button in (self.copy_button, self.save_button):
            button.setEnabled(False)
            button.setCursor(Qt.CursorShape.PointingHandCursor)
            actions.addWidget(button)
        content.addLayout(actions)
        body.addWidget(card, 1)
        layout.addLayout(body, 1)
        self.status_label = self._label("就绪 · 选择一个工具，开始截取屏幕。", "status")
        self.status_label.setWordWrap(True)
        self.status_label.setTextInteractionFlags(Qt.TextInteractionFlag.TextSelectableByMouse)
        layout.addWidget(self.status_label)

        for shortcut, callback in (
            ("Ctrl+Shift+A", self._begin_capture),
            ("Ctrl+S", self._save),
            ("Ctrl+C", self._copy),
            ("Ctrl+Q", self.quit_application),
        ):
            action = QAction(self)
            action.setShortcut(QKeySequence(shortcut))
            action.triggered.connect(callback)
            self.addAction(action)
        self._refresh_screens()
        app = QGuiApplication.instance()
        app.screenAdded.connect(self._refresh_screens)
        app.screenRemoved.connect(self._refresh_screens)
        self._setup_tray()
        self._setup_hotkey()

    @property
    def background_available(self) -> bool:
        return (
            self._tray is not None
            and self._tray.isVisible()
            and QSystemTrayIcon.isSystemTrayAvailable()
        )

    def _setup_tray(self) -> None:
        if not QSystemTrayIcon.isSystemTrayAvailable():
            self.background_button.setEnabled(False)
            self.background_button.setToolTip("当前桌面不支持系统托盘，关闭窗口将直接退出。")
            return
        icon = self.windowIcon()
        if icon.isNull():
            icon = self.style().standardIcon(QStyle.StandardPixmap.SP_DesktopIcon)
        self._tray = QSystemTrayIcon(icon, self)
        menu = QMenu(self)
        self._tray_capture_action = menu.addAction("开始截图")
        self._tray_capture_action.triggered.connect(self._capture_from_hotkey)
        menu.addAction("显示主窗口", self.show_main_window)
        menu.addSeparator()
        menu.addAction("退出 FreeCut", self.quit_application)
        self._tray.setContextMenu(menu)
        self._tray.setToolTip("FreeCut · 自由形状截图")
        self._tray.activated.connect(self._tray_activated)
        self._tray.messageClicked.connect(self.show_main_window)
        self._tray.show()
        QApplication.instance().setQuitOnLastWindowClosed(False)
        self.background_button.setToolTip("关闭主窗口也会隐藏到托盘；右键托盘图标可退出。")

    def _setup_hotkey(self) -> None:
        saved = self._settings.value("capture/hotkey", DEFAULT_HOTKEY, type=str)
        try:
            sequence = parse_hotkey(saved).text
        except ValueError:
            sequence = DEFAULT_HOTKEY
        self.hotkey_edit.setKeySequence(QKeySequence(sequence))
        if not self._hotkey.supported:
            self.hotkey_edit.setEnabled(False)
            self.hotkey_apply_button.setEnabled(False)
            self.hotkey_status_label.setText("全局快捷键仅支持 Windows。")
            return
        self._apply_hotkey(persist=False)

    def _apply_hotkey(self, checked: bool = False, *, persist: bool = True) -> None:
        try:
            sequence = self._hotkey.register(self.hotkey_edit.keySequence())
        except (ValueError, OSError) as error:
            previous = (
                f" 原快捷键 {self._hotkey.sequence} 仍有效。" if self._hotkey.sequence else ""
            )
            self.hotkey_status_label.setText(str(error) + previous)
            return
        self.hotkey_status_label.setText(f"{sequence} · 在其他应用中也能截图")
        self.capture_button.setToolTip(f"全局 {sequence}；窗口内 Ctrl+Shift+A")
        if self._tray_capture_action is not None:
            self._tray_capture_action.setText(f"开始截图  {sequence}")
            self._tray.setToolTip(f"FreeCut · {sequence} 截图")
        if persist:
            self._settings.setValue("capture/hotkey", sequence)
            self._settings.sync()
            if self._settings.status() != QSettings.Status.NoError:
                self.hotkey_status_label.setText(f"{sequence} 已生效，但设置保存失败。")

    def _tray_activated(self, reason: QSystemTrayIcon.ActivationReason) -> None:
        if reason in (
            QSystemTrayIcon.ActivationReason.Trigger,
            QSystemTrayIcon.ActivationReason.DoubleClick,
        ):
            self.show_main_window()

    def hide_to_tray(self) -> bool:
        if self._closing or self._capturing or not self.background_available:
            return False
        self.hide()
        if not self._background_notified:
            self._background_notified = True
            shortcut = self._hotkey.sequence
            tip = f"按 {shortcut} 直接截图。" if shortcut else "右键托盘图标可开始截图。"
            self._tray.showMessage("FreeCut 正在后台运行", tip + " 右键菜单可退出。")
        return True

    def show_main_window(self) -> None:
        if self._closing or self._capturing:
            return
        self.showNormal()
        self.raise_()
        self.activateWindow()

    def _capture_from_hotkey(self) -> None:
        if self._closing or self._capturing or QApplication.activeModalWidget() is not None:
            return
        screen = QGuiApplication.screenAt(QCursor.pos())
        # A background hotkey targets the monitor under the pointer. The capture
        # button continues to honor the explicit monitor selected in the UI.
        if screen is not None:
            for index in range(self.screen_combo.count()):
                if self.screen_combo.itemData(index) is screen:
                    self.screen_combo.setCurrentIndex(index)
                    break
        self._begin_capture(from_background=not self.isActiveWindow())

    def shutdown(self) -> None:
        """Release background resources before QApplication destroys the platform backend."""
        if self._closing:
            return
        self._closing = True
        self._capture_timer.stop()
        self._hotkey.close()
        if self._tray is not None:
            self._tray.hide()
        if self._overlay is not None:
            self._overlay.close()
        self._overlay = None
        self._target_screen = None
        self._capturing = False

    def quit_application(self) -> None:
        self.shutdown()
        self.close()

    @staticmethod
    def _label(text: str, name: str) -> QLabel:
        label = QLabel(text)
        label.setObjectName(name)
        return label

    def _set_mode(self, mode: Mode) -> None:
        self.mode = mode
        self.status_label.setText(f"已选择{mode.value} · 点击「开始截图」圈选屏幕内容。")

    def _refresh_screens(self, *args) -> None:
        previous = self.screen_combo.currentData()
        self.screen_combo.clear()
        for index, screen in enumerate(QGuiApplication.screens()):
            geometry = screen.geometry()
            self.screen_combo.addItem(
                f"屏幕 {index + 1} · {geometry.width()} × {geometry.height()}", screen
            )
            if screen is previous:
                self.screen_combo.setCurrentIndex(index)
        self.capture_button.setEnabled(not self._capturing and self.screen_combo.count() > 0)

    def _begin_capture(self, checked: bool = False, *, from_background: bool = False) -> None:
        if self._capturing or self._closing:
            return
        self._target_screen = self.screen_combo.currentData()
        if self._target_screen is None:
            self._show_error("当前没有可截取的屏幕。")
            return
        self._capturing = True
        self._return_to_background = self.background_available and (
            from_background or not self.isVisible() or self.isMinimized()
        )
        self.capture_button.setEnabled(False)
        self.hide()
        self._capture_timer.start(self.delay_combo.currentData())

    def _take_capture(self) -> None:
        try:
            screen = self._target_screen
            if screen not in QGuiApplication.screens():
                raise RuntimeError("目标屏幕已断开，请重新选择显示器。")
            image = screen.grabWindow(0).toImage()
            if image.isNull():
                raise RuntimeError("无法获取屏幕画面。请检查系统截图权限后重试。")
            self._overlay = SelectionOverlay(image, self.mode, screen.geometry())
            self._overlay.setScreen(screen)
            self._overlay.setGeometry(screen.geometry())
            self._overlay.accepted.connect(self._on_capture)
            self._overlay.cancelled.connect(self._on_cancel)
            self._overlay.show()
            self._overlay.raise_()
            self._overlay.activateWindow()
            self._overlay.setFocus()
        except RuntimeError as error:
            self._restore(show_window=True)
            self._show_error(str(error))

    def _restore(self, *, show_window: bool = True) -> None:
        self._capturing = False
        self._overlay = None
        self._target_screen = None
        if not self._closing:
            self._refresh_screens()
            if show_window or not self.background_available:
                self.show_main_window()

    def _on_capture(self, image: QImage) -> None:
        self.image = image.copy()
        self.preview.set_image(image)
        self.dimensions_label.setText(f"{image.width()} × {image.height()} px")
        self.copy_button.setEnabled(True)
        self.save_button.setEnabled(True)
        self.status_label.setText("截图完成 · 可以复制图片，或保存为透明 PNG。")
        self._restore(show_window=True)

    def _on_cancel(self) -> None:
        self.status_label.setText("已取消本次截图。")
        self._restore(show_window=not self._return_to_background)

    def _copy(self) -> None:
        if self.image.isNull():
            return
        try:
            QApplication.clipboard().setMimeData(image_mime_data(self.image))
            self.status_label.setText("已复制图片 · 可粘贴到支持图片的应用。")
        except (ValueError, OSError):
            self._show_error("复制失败，请尝试保存 PNG 文件。")

    def _save(self) -> None:
        if self.image.isNull():
            return
        directory = QStandardPaths.writableLocation(
            QStandardPaths.StandardLocation.PicturesLocation
        )
        default = Path(directory or str(Path.home())) / datetime.now().strftime(
            "FreeCut-%Y%m%d-%H%M%S.png"
        )
        filename, _ = QFileDialog.getSaveFileName(
            self, "保存透明截图", str(default), "PNG 图片 (*.png)"
        )
        if not filename:
            return
        path = Path(filename)
        if path.suffix.lower() != ".png":
            path = path.with_name(path.name + ".png")
            if (
                path.exists()
                and QMessageBox.question(self, "替换文件", f"文件 {path.name} 已存在，是否替换？")
                != QMessageBox.StandardButton.Yes
            ):
                return
        try:
            saved = save_png(self.image, path)
            self.status_label.setText(f"已保存：{saved}")
        except (ValueError, OSError) as error:
            self._show_error(str(error))

    def _show_error(self, message: str) -> None:
        self.status_label.setText(message)
        QMessageBox.warning(self, "FreeCut", message)

    def closeEvent(self, event) -> None:
        if not self._closing and self.hide_to_tray():
            event.ignore()
            return
        self.shutdown()
        event.accept()
        QApplication.instance().quit()
