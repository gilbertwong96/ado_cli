// ado landing page — turn raw <pre><code> blocks into styled "shell" blocks
// with traffic lights + copy-to-clipboard button.
//
// Run on DOMContentLoaded. Scoped to the page (not a global).
// Idempotent: re-running is safe (data-shell-wrapped attribute prevents loops).
//
(function () {
  "use strict";

  function wrapShellBlocks() {
    var blocks = document.querySelectorAll("pre > code");

    for (var i = 0; i < blocks.length; i++) {
      var pre = blocks[i].parentNode;
      if (!pre || pre.parentNode == null) continue;

      // Skip if already wrapped.
      if (pre.parentNode.classList && pre.parentNode.classList.contains("shell-block")) continue;
      if (pre.dataset.shellWrapped === "1") continue;

      var shell = document.createElement("div");
      shell.className = "shell-block";
      shell.setAttribute("role", "group");
      shell.setAttribute("aria-label", "Code block");

      // Capture the original parent BEFORE we move pre into shell.
      // Otherwise pre.parentNode would be `shell` (the new parent),
      // and insertBefore would nest shell inside itself.
      var originalParent = pre.parentNode;

      // Mark the <pre> as wrapped so we don't loop, then move it into
      // the shell block. Also ensure the <pre> gets the .shell-content
      // class.
      pre.dataset.shellWrapped = "1";
      pre.classList.add("shell-content");

      var header = document.createElement("div");
      header.className = "shell-header";

      var dots = document.createElement("div");
      dots.className = "shell-dots";
      dots.innerHTML =
        '<span class="dot dot-red" aria-hidden="true"></span>' +
        '<span class="dot dot-yellow" aria-hidden="true"></span>' +
        '<span class="dot dot-green" aria-hidden="true"></span>';

      var label = document.createElement("span");
      label.className = "shell-label";
      label.textContent = guessShellLabel(pre);

      var btn = document.createElement("button");
      btn.type = "button";
      btn.className = "copy-btn";
      btn.setAttribute("aria-label", "Copy code to clipboard");
      btn.textContent = "Copy";

      header.appendChild(dots);
      header.appendChild(label);
      header.appendChild(btn);

      shell.appendChild(header);

      // Insert shell at pre's current position in originalParent. Pass
      // `null` as the reference (not `pre`) so insertBefore appends to
      // the end of originalParent. Then move pre into shell AFTER the
      // insert — otherwise pre would no longer be a child of
      // originalParent and insertBefore would throw NotFoundError.
      originalParent.insertBefore(shell, null);
      shell.appendChild(pre);

      attachCopyHandler(btn, pre);
    }
  }

  // Pick a label for the shell header based on the first non-empty line
  // of the code (e.g. "shell", "json", "ado"). Falls back to "code".
  function guessShellLabel(pre) {
    var text = (pre.textContent || "").trim();
    if (!text) return "code";
    var firstLine = text.split("\n")[0].trim();
    if (firstLine.indexOf("$ ") === 0) {
      return "shell";
    }
    if (firstLine.charAt(0) === "{" || firstLine.charAt(0) === "[") {
      return "json";
    }
    if (firstLine.indexOf("# ") === 0) {
      return "shell";
    }
    if (firstLine.indexOf("git ") === 0 ||
        firstLine.indexOf("curl ") === 0 ||
        firstLine.indexOf("npm ") === 0 ||
        firstLine.indexOf("brew ") === 0 ||
        firstLine.indexOf("mix ") === 0 ||
        firstLine.indexOf("./") === 0 ||
        firstLine.indexOf("ado ") === 0) {
      return "shell";
    }
    return "code";
  }

  function attachCopyHandler(btn, pre) {
    btn.addEventListener("click", function () {
      var text = pre.textContent || "";
      copyToClipboard(text).then(function (ok) {
        if (ok) {
          btn.textContent = "Copied!";
          btn.classList.add("copied");
        } else {
          btn.textContent = "Failed";
        }
        setTimeout(function () {
          btn.textContent = "Copy";
          btn.classList.remove("copied");
        }, 1500);
      });
    });
  }

  // Modern Clipboard API + textarea fallback for old browsers / non-HTTPS.
  function copyToClipboard(text) {
    if (navigator.clipboard && window.isSecureContext) {
      return navigator.clipboard.writeText(text).then(
        function () { return true; },
        function () { return fallbackCopy(text); }
      );
    }
    return Promise.resolve(fallbackCopy(text));
  }

  function fallbackCopy(text) {
    try {
      var ta = document.createElement("textarea");
      ta.value = text;
      ta.style.position = "fixed";
      ta.style.left = "-9999px";
      document.body.appendChild(ta);
      ta.focus();
      ta.select();
      var ok = document.execCommand("copy");
      document.body.removeChild(ta);
      return ok;
    } catch (e) {
      return false;
    }
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", wrapShellBlocks);
  } else {
    wrapShellBlocks();
  }
})();
