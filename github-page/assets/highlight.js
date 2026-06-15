// ado landing page — subtle JSON syntax highlighter.
//
// For each shell-block, scan its code content line by line. Lines
// that look like JSON (contain `{`, `}`, `[`, `]`, or start with
// a quoted key) get tokenized. Lines that look like shell (start
// with `$ `) are left untouched.
//
// Token colors (see style.css):
//   .tok-key   — JSON keys ("foo":)         blue
//   .tok-str   — string values              cyan
//   .tok-bool  — true / false / null        orange
//   .tok-num   — numbers                    purple
//   .tok-punct — { } [ ] ,                  muted gray

(function () {
  "use strict";

  function highlightAll() {
    var codes = document.querySelectorAll(".shell-block code");
    for (var i = 0; i < codes.length; i++) {
      highlightOne(codes[i]);
    }
  }

  function highlightOne(code) {
    if (code.dataset.highlighted === "1") return;
    if (code.hasAttribute("data-no-highlight")) return;

    var raw = code.textContent || "";
    // Preserve the original prompt span if present
    var promptMatch = raw.match(/^(\s*\$\s*)/);

    // Split into lines, process each, then rejoin
    var lines = raw.split("\n");
    var processed = lines.map(function (line) {
      return processLine(line);
    });
    var html = processed.join("\n");

    // If the original had a prompt, wrap it in a span so the green
    // color is preserved (otherwise escapeHtml would convert $ to &#36;).
    if (promptMatch) {
      // Re-extract the prompt and the rest
      var prompt = promptMatch[1];
      var rest = html.substring(prompt.length);
      code.innerHTML =
        "<span class=\"prompt\">" + prompt.replace(/ /g, "&nbsp;") + "</span>" +
        (rest.charAt(0) === " " ? "&nbsp;" : "") +
        rest.replace(/^ /, "");
    } else {
      code.innerHTML = html;
    }
    code.dataset.highlighted = "1";
  }

  // Process one line. If it looks like JSON, tokenize it. Otherwise
  // return it as plain HTML-escaped text.
  function processLine(line) {
    // Shell command line: "$ foo --bar"  → escape and return
    if (/^\s*\$/.test(line)) return escapeHtml(line);
    // Comment line: "# ..." or starting with //  → escape
    if (/^\s*#/.test(line)) return escapeHtml(line);

    // JSON-like line: has { } [ ] or "key":
    if (!/[{}\[\]]/.test(line) && !/\"\w+\"\s*:/.test(line)) {
      return escapeHtml(line);
    }

    // Tokenize the JSON part
    return tokenizeJsonLine(line);
  }

  // Tokenize a line that's expected to contain JSON tokens.
  // Walk character by character, matching strings, numbers,
  // booleans, null, and punctuation.
  function tokenizeJsonLine(line) {
    var out = "";
    var i = 0;
    var len = line.length;
    while (i < len) {
      var c = line.charAt(i);

      // Whitespace: preserve as-is (don't escape)
      if (/\s/.test(c)) {
        out += c;
        i++;
        continue;
      }

      // String (with possible JSON key/value content)
      if (c === "\"") {
        var end = findStringEnd(line, i);
        var str = line.substring(i, end);
        // Is this a JSON key? Look for the next non-whitespace char.
        var after = line.substring(end);
        if (/^\s*:/.test(after)) {
          out += "<span class=\"tok-key\">" + escapeHtml(str) + "</span>";
        } else {
          out += "<span class=\"tok-str\">" + escapeHtml(str) + "</span>";
        }
        i = end;
        continue;
      }

      // Number
      if (c >= "0" && c <= "9") {
        var end = i;
        while (end < len && /[0-9.\-eE+]/.test(line.charAt(end))) end++;
        out += "<span class=\"tok-num\">" + escapeHtml(line.substring(i, end)) + "</span>";
        i = end;
        continue;
      }

      // Booleans / null
      if (c === "t" && line.substr(i, 4) === "true") {
        out += "<span class=\"tok-bool\">true</span>";
        i += 4;
        continue;
      }
      if (c === "f" && line.substr(i, 5) === "false") {
        out += "<span class=\"tok-bool\">false</span>";
        i += 5;
        continue;
      }
      if (c === "n" && line.substr(i, 4) === "null") {
        out += "<span class=\"tok-bool\">null</span>";
        i += 4;
        continue;
      }

      // Punctuation: { } [ ] ,
      if ("{}[],".indexOf(c) !== -1) {
        out += "<span class=\"tok-punct\">" + c + "</span>";
        i++;
        continue;
      }

      // Other characters: escape and preserve
      out += escapeHtml(c);
      i++;
    }
    return out;
  }

  // Find the end of a JSON string starting at the opening quote.
  function findStringEnd(text, start) {
    var i = start + 1;
    var len = text.length;
    while (i < len) {
      var c = text.charAt(i);
      if (c === "\\") { i += 2; continue; }
      if (c === "\"") return i + 1;
      i++;
    }
    return len;
  }

  function escapeHtml(s) {
    return s
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;")
      .replace(/'/g, "&#39;");
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", highlightAll);
  } else {
    highlightAll();
  }
})();
