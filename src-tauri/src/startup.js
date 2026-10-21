// Installed before page scripts so failed imports and old runtimes are reported too.
(function () {
  var reported = false;
  function report(message) {
    if (reported) return;
    reported = true;
    var api = window.__TAURI_INTERNALS__;
    if (api && typeof api.invoke === "function") {
      api
        .invoke("frontend_failed", { message: String(message).slice(0, 4000) })
        .catch(function () {});
    }
  }
  window.addEventListener(
    "error",
    function (event) {
      if (event.message) {
        report((event.error && event.error.stack) || event.message);
      } else if (event.target && event.target.tagName === "SCRIPT") {
        report("无法加载界面脚本。");
      }
    },
    true,
  );
  window.addEventListener("unhandledrejection", function (event) {
    report((event.reason && event.reason.stack) || String(event.reason));
  });
})();
