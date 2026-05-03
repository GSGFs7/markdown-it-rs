pub const DEMO_CSS: &str = r#"
:root {
  color-scheme: light;
  --fg: #1f2328;
  --muted: #59636e;
  --border: #d1d9e0;
  --canvas: #ffffff;
  --canvas-subtle: #f6f8fa;
  --note: #0969da;
  --tip: #1a7f37;
  --important: #8250df;
  --warning: #9a6700;
  --caution: #d1242f;
}

* { box-sizing: border-box; }
body {
  margin: 0;
  color: var(--fg);
  background: var(--canvas-subtle);
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  line-height: 1.5;
}

main {
  width: min(880px, calc(100% - 32px));
  margin: 40px auto;
  padding: 28px;
  background: var(--canvas);
  border: 1px solid var(--border);
  border-radius: 8px;
}

h1 { margin: 0 0 16px; font-size: 28px; border-bottom: 1px solid var(--border); padding-bottom: 8px; }
h2 { margin: 32px 0 16px; font-size: 20px; color: var(--muted); }

/* --- Text Directive: Badge --- */
.badge {
  display: inline-block;
  padding: 0 7px;
  font-size: 12px;
  font-weight: 500;
  line-height: 18px;
  white-space: nowrap;
  border: 1px solid transparent;
  border-radius: 2em;
  margin: 0 2px;
  vertical-align: middle;
}
.badge-info { background: #ddf4ff; color: #0969da; border-color: #54aeff; }
.badge-success { background: #dafbe1; color: #1a7f37; border-color: #4ac26b; }
.badge-error { background: #ffebe9; color: #d1242f; border-color: #ff8182; }

/* --- Leaf Directive: Youtube --- */
.video-container {
  position: relative;
  width: 100%;
  padding-bottom: 56.25%; /* 16:9 */
  margin: 16px 0;
}
.video-container iframe {
  position: absolute;
  top: 0; left: 0; width: 100%; height: 100%;
  border-radius: 6px;
  border: 1px solid var(--border);
}

/* --- Container Directive: Alerts --- */
.markdown-alert {
  margin: 16px 0;
  padding: 8px 16px;
  border-left: 4px solid var(--border);
}
.markdown-alert > :last-child { margin-bottom: 0; }
.markdown-alert-title { display: flex; align-items: center; margin: 0 0 8px; font-weight: 600; }
.markdown-alert-note { border-left-color: var(--note); }
.markdown-alert-note .markdown-alert-title { color: var(--note); }
.markdown-alert-tip { border-left-color: var(--tip); }
.markdown-alert-tip .markdown-alert-title { color: var(--tip); }
.markdown-alert-important { border-left-color: var(--important); }
.markdown-alert-important .markdown-alert-title { color: var(--important); }
.markdown-alert-warning { border-left-color: var(--warning); }
.markdown-alert-warning .markdown-alert-title { color: var(--warning); }
.markdown-alert-caution { border-left-color: var(--caution); }
.markdown-alert-caution .markdown-alert-title { color: var(--caution); }
"#;
