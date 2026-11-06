use serde_json::Value;

pub(super) const SNAPSHOT_ELEMENT_LIMIT: usize = 200;

const PAGE_HELPERS: &str = r#"
const zoraiInteractive = 'a[href],button,input:not([type=hidden]),textarea,select,summary,[role=button],[role=link],[role=checkbox],[role=radio],[role=tab],[role=menuitem],[role=option],[role=switch],[contenteditable=""],[contenteditable=true],[onclick]';
const zoraiVisible = (el) => {
  const rect = el.getBoundingClientRect();
  const style = getComputedStyle(el);
  return rect.width > 0 && rect.height > 0 && style.visibility !== 'hidden' && style.display !== 'none';
};
const zoraiName = (el) => {
  const isPassword = el.getAttribute('type') === 'password';
  const raw = el.getAttribute('aria-label') || el.innerText || (isPassword ? '' : el.value) || el.getAttribute('placeholder') || el.getAttribute('title') || el.getAttribute('alt') || el.getAttribute('name') || '';
  return String(raw).trim().replace(/\s+/g, ' ').slice(0, 80);
};
const zoraiDescribe = (el) => `${el.tagName.toLowerCase()}${el.dataset.zoraiRef ? ` [${el.dataset.zoraiRef}]` : ''} "${zoraiName(el)}"`;
const zoraiFind = (target) => {
  if (target.ref) return document.querySelector(`[data-zorai-ref="${CSS.escape(String(target.ref))}"]`);
  if (target.selector) return document.querySelector(target.selector);
  if (target.text) {
    const needle = String(target.text).toLowerCase();
    const candidates = [...document.querySelectorAll(zoraiInteractive)].filter(zoraiVisible);
    return candidates.find((el) => zoraiName(el).toLowerCase() === needle)
      || candidates.find((el) => zoraiName(el).toLowerCase().includes(needle))
      || null;
  }
  return null;
};
const zoraiMissing = (target) => JSON.stringify({ ok: false, error: `No element matches ${JSON.stringify(target)}. Call browser_read_dom to refresh element refs.` });
"#;

fn literal(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

fn wrap(body: &str) -> String {
    format!("(() => {{{PAGE_HELPERS}\n{body}\n}})()")
}

pub(super) fn page_info_script() -> String {
    "JSON.stringify({ url: location.href, title: document.title })".to_string()
}

pub(super) fn ready_state_script() -> String {
    "document.readyState".to_string()
}

pub(super) fn snapshot_script(
    max_chars: usize,
    selector: Option<&str>,
    include_text: bool,
) -> String {
    wrap(&format!(
        r#"
const maxChars = {max_chars};
const scope = {selector};
const roots = scope ? [...document.querySelectorAll(scope)] : [document];
let next = Number(document.documentElement.dataset.zoraiRefNext || 1);
const elements = [];
for (const root of roots) {{
  const candidates = root === document ? document.querySelectorAll(zoraiInteractive) : [root, ...root.querySelectorAll(zoraiInteractive)];
  for (const el of candidates) {{
    if (!(el instanceof Element) || !el.matches(zoraiInteractive) || !zoraiVisible(el)) continue;
    if (!el.dataset.zoraiRef) el.dataset.zoraiRef = String(next++);
    elements.push({{
      ref: el.dataset.zoraiRef,
      tag: el.tagName.toLowerCase(),
      type: el.getAttribute('type') || undefined,
      role: el.getAttribute('role') || undefined,
      name: zoraiName(el),
      href: el.getAttribute('href') || undefined,
      disabled: el.disabled || el.getAttribute('aria-disabled') === 'true' || undefined,
    }});
    if (elements.length >= {limit}) break;
  }}
}}
document.documentElement.dataset.zoraiRefNext = String(next);
const text = {include_text} && document.body ? document.body.innerText.replace(/\n{{3,}}/g, '\n\n').trim() : '';
return JSON.stringify({{ url: location.href, title: document.title, text: text.slice(0, maxChars), truncated: text.length > maxChars, elements }});
"#,
        selector = literal(&selector),
        limit = SNAPSHOT_ELEMENT_LIMIT,
    ))
}

pub(super) fn click_script(target: &Value) -> String {
    wrap(&format!(
        r#"
const target = {target};
const el = zoraiFind(target);
if (!el) return zoraiMissing(target);
el.scrollIntoView({{ block: 'center', inline: 'center' }});
if (typeof el.focus === 'function') el.focus();
el.click();
return JSON.stringify({{ ok: true, clicked: zoraiDescribe(el), url: location.href }});
"#,
        target = literal(target),
    ))
}

pub(super) fn type_script(target: &Value, text: &str, clear: bool, submit: bool) -> String {
    wrap(&format!(
        r#"
const target = {target};
const text = {text};
const el = zoraiFind(target);
if (!el) return zoraiMissing(target);
el.scrollIntoView({{ block: 'center' }});
el.focus();
if (el.isContentEditable) {{
  if ({clear}) {{
    document.execCommand('selectAll', false);
    document.execCommand('delete', false);
  }}
  document.execCommand('insertText', false, text);
}} else if ('value' in el) {{
  const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype
    : el instanceof HTMLSelectElement ? HTMLSelectElement.prototype
    : HTMLInputElement.prototype;
  const setter = Object.getOwnPropertyDescriptor(proto, 'value').set;
  setter.call(el, {clear} ? text : `${{el.value}}${{text}}`);
  el.dispatchEvent(new Event('input', {{ bubbles: true }}));
  el.dispatchEvent(new Event('change', {{ bubbles: true }}));
}} else {{
  return JSON.stringify({{ ok: false, error: `${{zoraiDescribe(el)}} does not accept text input.` }});
}}
if ({submit}) {{
  const init = {{ key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true, cancelable: true }};
  const proceed = el.dispatchEvent(new KeyboardEvent('keydown', init));
  el.dispatchEvent(new KeyboardEvent('keyup', init));
  if (proceed && el.form) {{
    if (typeof el.form.requestSubmit === 'function') el.form.requestSubmit();
    else el.form.submit();
  }}
}}
return JSON.stringify({{ ok: true, typed_into: zoraiDescribe(el), submitted: {submit}, url: location.href }});
"#,
        target = literal(target),
        text = literal(&text),
    ))
}

pub(super) fn scroll_script(
    target: Option<&Value>,
    direction: &str,
    amount: Option<i64>,
) -> String {
    wrap(&format!(
        r#"
const target = {target};
if (target) {{
  const el = zoraiFind(target);
  if (!el) return zoraiMissing(target);
  el.scrollIntoView({{ block: 'center' }});
}} else {{
  const step = {amount} ?? Math.round(window.innerHeight * 0.8);
  const direction = {direction};
  if (direction === 'top') window.scrollTo(0, 0);
  else if (direction === 'bottom') window.scrollTo(0, document.documentElement.scrollHeight);
  else window.scrollBy(0, direction === 'up' ? -step : step);
}}
return JSON.stringify({{
  ok: true,
  scroll_y: Math.round(window.scrollY),
  viewport_height: window.innerHeight,
  page_height: document.documentElement.scrollHeight,
}});
"#,
        target = literal(&target),
        amount = literal(&amount),
        direction = literal(&direction),
    ))
}
