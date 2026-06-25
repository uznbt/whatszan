console.log("injected.js");

function ewHijackNotif(prefix) {
  window.realNotification = window.Notification;

  const override = {
    construct(target, args) {
      args[0] = `${prefix}${args[0]}`;
      const thing = new target(...args);
      thing.addEventListener("click", (ev) => {
        console.log("ev", ev);
        window?.ipc?.notifyEv?.(JSON.stringify(ev));
      });

      return thing;
    },
  };

  window.Notification = new Proxy(window.realNotification, override);
}

function ewHijackClick() {
  document.body.addEventListener("click", (ev) => {
    if (!(ev.target instanceof HTMLAnchorElement)) return;
    if (ev.target.tagName === "A" && ev.target.getAttribute("target") === "_blank") {
      const href = ev.target.href;
      if (href && (href.includes("chat.whatsapp.com") || href.includes("api.whatsapp.com") || href.includes("wa.me"))) {
        return; // Let WhatsApp Web handle its own internal links natively
      }
      ev.preventDefault();
      window?.ipc?.open?.(href);
    }
  });
}

function ewDoWhatsappAction(whatsappAction) {
  if (ewDoWhatsappAction.wa == null) {
    if (importDefault) {
      ewDoWhatsappAction.wa = importDefault("WAWebKeyboardRun");
    }
  }
  ewDoWhatsappAction.wa?.(whatsappAction);
}

async function ewSetupKeys() {
  const keys = await window?.ipc?.stateGet("keys");
  const kMap = await navigator.keyboard?.getLayoutMap?.();

  const parsedCache = {};
  function parseBinding(binding) {
    if (binding in parsedCache) {
      return parsedCache[binding];
    }
    const [modifiers, key] = binding.split(" ");
    let mapped;
    // if key is digit, use correct key for other keyboard layouts (like azerty)
    if (kMap && key.length == 1 && key >= "0" && key <= "9") {
      mapped = kMap.get(`Digit${key}`);
      console.debug("mapped", key, "to", mapped);
    }

    const parsed = {
      key: mapped ?? key,
      ctrlKey: modifiers.includes("C"),
      shiftKey: modifiers.includes("S"),
      altKey: modifiers.includes("A"),
      metaKey: modifiers.includes("M"),
    };
    parsedCache[binding] = parsed;
    return parsed;
  }

  console.debug("keys", keys);

  async function doAction(effect) {
    await {
      OPEN_NTH_CHAT: (effect) => openNthChat(effect.chatIndex),
      SLEEP: async (effect) => new Promise((resolve) => setTimeout(resolve, effect.duration)),
      IPC: async (effect) => window?.ipc?.[effect.method]?.(...(effect.args ?? [])),
    }[effect.action]?.(effect);
  }

  function openNthChat(chatIndex) {
    doAction.WAWebCmd ??= require("WAWebCmd");
    doAction.WAWebChatCollection ??= require("WAWebChatCollection");

    let skip = 0;
    for (let i = 0; i <= chatIndex; i++) {
      while (doAction.WAWebChatCollection?.ChatCollection._models[i + skip]?.__x_archive) {
        skip++;
      }
    }
    const chat = doAction.WAWebChatCollection?.ChatCollection._models[chatIndex + skip] ?? null;

    if (chat !== null) {
      chat.chat = chat;
      doAction.WAWebCmd?.Cmd.openChatBottom(chat);
    }
  }

  async function executeEffect(effect) {
    if (typeof effect?.whatsappAction === "string") {
      ewDoWhatsappAction(effect.whatsappAction);
    } else if (typeof effect?.action === "string") {
      await doAction(effect);
    }
  }

  // single effect (dict) or array of effects
  async function executeEffects(effects) {
    const effectsArray = Array.isArray(effects) ? effects : [effects];

    for (let i = 0; i < effectsArray.length; i++) {
      console.log("executeEffect", effectsArray[i]);
      await executeEffect(effectsArray[i]);
    }
  }

  addEventListener("keydown", async (ev) => {
    for (let [binding, effect] of Object.entries(keys)) {
      const parsed = parseBinding(binding);
      let match = true;
      for (let [k, v] of Object.entries(parsed)) {
        match &&= ev?.[k] == v;
      }
      if (match) {
        console.log("effect", effect);
        await executeEffects(effect);
      }
    }
  });
}

function ewReplaceLogo() {
  const processTextNode = (node) => {
    if (node.nodeValue === 'WhatsApp' || node.nodeValue === 'WhatsApp Web') {
      let parent = node.parentElement;
      let isMessage = false;
      while (parent) {
        // Safe check using getAttribute for elements
        if (parent.getAttribute && (parent.getAttribute('data-testid') === 'msg-container' || parent.getAttribute('role') === 'row' || parent.id === 'main')) {
          isMessage = true;
          break;
        }
        parent = parent.parentElement;
      }
      if (!isMessage) {
        node.nodeValue = node.nodeValue.replace('WhatsApp', 'WhatsZan');
      }
    }
  };

  const processElement = (root) => {
    // 1. Check for SVGs inside this element
    const svgs = root.querySelectorAll ? root.querySelectorAll('svg[aria-label="WhatsApp"]') : [];
    if (root.tagName === 'svg' && root.getAttribute('aria-label') === 'WhatsApp') svgs.push(root);
    
    svgs.forEach(svg => {
      const parent = svg.parentElement;
      if (parent && !parent.querySelector('.whatszan-logo')) {
        svg.style.display = 'none';
        const span = document.createElement('span');
        span.className = 'whatszan-logo';
        span.textContent = 'WhatsZan';
        span.style.fontWeight = 'bold';
        span.style.fontSize = '18px';
        span.style.color = 'inherit'; 
        parent.appendChild(span);
      }
    });

    // 2. Banner Killer
    const text = root.innerText || "";
    if (((text.includes('Unduh WhatsApp untuk Windows') || text.includes('Unduh WhatsZan untuk Windows')) && text.includes('fitur ekstra')) || 
        text.includes('Dapatkan WhatsApp untuk Windows') || text.includes('Dapatkan WhatsZan untuk Windows')) {
      root.style.display = 'none';
      let parent = root.parentElement;
      for (let i = 0; i < 3; i++) {
        if (parent && parent.tagName !== 'BODY') {
          const rect = parent.getBoundingClientRect();
          if (rect.height > 40 && rect.height < 250) {
            parent.style.display = 'none';
          }
          parent = parent.parentElement;
        }
      }
    }

    // 3. Process all text nodes inside this element
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, null, false);
    let tNode;
    while (tNode = walker.nextNode()) {
      processTextNode(tNode);
    }
  };

  // Run once immediately on existing body
  processElement(document.body);

  // Instantly process all new mutations
  const uiObserver = new MutationObserver((mutations) => {
    for (const mutation of mutations) {
      if (mutation.type === 'childList') {
        for (const node of mutation.addedNodes) {
          if (node.nodeType === Node.ELEMENT_NODE) {
            processElement(node);
          } else if (node.nodeType === Node.TEXT_NODE) {
            processTextNode(node);
          }
        }
      } else if (mutation.type === 'characterData') {
        processTextNode(mutation.target);
      }
    }
  });

  uiObserver.observe(document.body, { childList: true, subtree: true, characterData: true });
}

function ewHijackTitle() {
  // Ensure the document title also says WhatsZan instead of WhatsApp
  const titleEl = document.querySelector('title');
  if (titleEl) {
    new MutationObserver(() => {
      if (document.title.includes('WhatsApp')) {
        document.title = document.title.replace('WhatsApp', 'WhatsZan');
      }
    }).observe(titleEl, { childList: true });
    // Trigger once initially
    if (document.title.includes('WhatsApp')) {
      document.title = document.title.replace('WhatsApp', 'WhatsZan');
    }
  }
}

async function ewSetupDictionary() {
  const dict = await window?.ipc?.stateGet?.("textReplacements");
  if (!dict || Object.keys(dict).length === 0) return;

  const triggers = Object.keys(dict);

  document.addEventListener('keydown', (ev) => {
    const target = ev.target;
    // Only process single character keys
    if (target.isContentEditable && ev.key.length === 1) {
      const selection = window.getSelection();
      if (!selection.rangeCount) return;

      const range = selection.getRangeAt(0);
      if (!range.collapsed) return;

      const node = range.startContainer;
      if (node.nodeType === Node.TEXT_NODE) {
        // Simulate what the text WOULD be after this keystroke
        const textBeforeCursor = node.textContent.slice(0, range.startOffset) + ev.key;

        for (const trigger of triggers) {
          const actualTrigger = trigger.startsWith('/') ? trigger : '/' + trigger;

          if (textBeforeCursor.endsWith(actualTrigger)) {
            ev.preventDefault(); // Stop the last character from being typed naturally

            // Select the characters of the trigger that are ALREADY in the DOM
            const triggerWithoutLastChar = actualTrigger.slice(0, -1);
            const startOffset = range.startOffset - triggerWithoutLastChar.length;

            if (startOffset >= 0) {
              const replaceRange = document.createRange();
              replaceRange.setStart(node, startOffset);
              replaceRange.setEnd(node, range.startOffset);
              
              selection.removeAllRanges();
              selection.addRange(replaceRange);
              
              // Insert the full replacement text (React will catch this as an input event)
              document.execCommand('insertText', false, dict[trigger]);
            }
            break;
          }
        }
      }
    }
  }, { capture: true });
}

const savedMessages = new Map();

function ewSetupAntiDelete() {
  console.log("Anti-Delete setup initialized.");
  
  const observer = new MutationObserver((mutations) => {
    for (const mut of mutations) {
      if (mut.type === 'childList') {
        mut.addedNodes.forEach(node => {
          if (node.nodeType === Node.ELEMENT_NODE) {
            // Check for new messages OR updates inside existing messages
            const msgs = new Set();
            if (node.closest && node.closest('div[data-id]')) {
              msgs.add(node.closest('div[data-id]'));
            }
            if (node.matches && node.matches('div[data-id]')) {
              msgs.add(node);
            }
            if (node.querySelectorAll) {
              node.querySelectorAll('div[data-id]').forEach(m => msgs.add(m));
            }
            
            msgs.forEach(msg => {
              const id = msg.getAttribute('data-id');
              if (!id) return;
              
              const isOut = id.startsWith('true_'); 
              if (isOut) return; // We only care about incoming messages
              
              const textNode = msg.querySelector('.copyable-text span.selectable-text') || msg.querySelector('span.selectable-text[dir="ltr"]') || msg.querySelector('span.selectable-text');
              const imgNode = msg.querySelector('img[src^="blob:"]');
              
              // Save message if it's new
              if (!savedMessages.has(id) && (textNode || imgNode)) {
                // Jangan simpan kalau text-nya ternyata tulisan 'dihapus'
                const textContent = textNode ? textNode.innerText || textNode.textContent : '';
                if (!textContent.includes('Pesan ini dihapus') && !textContent.includes('telah dihapus') && !msg.querySelector('span[data-icon="recalled"]')) {
                  savedMessages.set(id, {
                    text: textNode ? textNode.innerHTML : '',
                    img: imgNode ? imgNode.src : null,
                    timestamp: Date.now()
                  });
                }
              }
              
              // Prevent memory leak (max 2000 messages in RAM)
              if (savedMessages.size > 2000) {
                const firstKey = savedMessages.keys().next().value;
                savedMessages.delete(firstKey);
              }
              
              // Check if this is a deleted message tombstone
              const isDeleted = msg.querySelector('span[data-icon="recalled"]') || 
                                (msg.innerText && (msg.innerText.includes('telah dihapus') || msg.innerText.includes('was deleted') || msg.innerText.includes('Pesan ini dihapus')));
                                
              // ONLY recover if the setting is currently ON
              if (isDeleted && savedMessages.has(id) && window.wzIncognito?.antiDelete) {
                const saved = savedMessages.get(id);
                if (!msg.querySelector('.wz-recovered')) {
                  const recoveredDiv = document.createElement('div');
                  recoveredDiv.className = 'wz-recovered';
                  recoveredDiv.style.cssText = 'color: #ff3b30; font-style: italic; font-size: 0.9em; margin-top: 4px; padding-top: 4px; border-top: 1px solid rgba(255,59,48,0.3); line-height: 1.4;';
                  
                  let content = `🚫 <b>Pesan Dipulihkan:</b><br/>${saved.text}`;
                  if (saved.img) {
                    content += `<br/><a href="${saved.img}" target="_blank" style="color:#007aff; text-decoration:underline;">[Lihat Media]</a>`;
                  }
                  
                  recoveredDiv.innerHTML = content;
                  
                  const innerBox = msg.querySelector('.copyable-text')?.parentElement || msg.firstElementChild;
                  if (innerBox) {
                    innerBox.appendChild(recoveredDiv);
                  }
                }
              }
            });
          }
        });
      }
    }
  });

  observer.observe(document.body, { childList: true, subtree: true });
}

function ewSetupStatusSaver() {
  console.log("Status Saver setup initialized.");
  const btn = document.createElement('button');
  btn.innerHTML = 'Simpan Status';
  btn.style.cssText = 'position: fixed !important; top: 20px !important; right: 80px !important; z-index: 2147483647 !important; padding: 8px 16px !important; background: #25D366 !important; color: white !important; border: none !important; border-radius: 20px !important; font-weight: bold !important; cursor: pointer !important; display: none; box-shadow: 0 4px 12px rgba(0,0,0,0.5) !important; transition: background 0.2s !important;';
  
  btn.onmouseover = () => btn.style.setProperty('background', '#1da851', 'important');
  btn.onmouseout = () => btn.style.setProperty('background', '#25D366', 'important');
  
  document.body.appendChild(btn);

  let activeStatusUrl = null;

  setInterval(() => {
    let isStatus = false;
    let targetMedia = null;
    
    // Check if we are in status viewer (usually dark background, huge media)
    // We can just find the largest media on screen. If it takes up > 40% of window height, it's likely a status or full-screen media viewer.
    const medias = Array.from(document.querySelectorAll('video, img'));
    let maxArea = 0;
    
    for (const m of medias) {
      const rect = m.getBoundingClientRect();
      
      // Ignore tiny icons or avatars
      if (rect.width < 100 || rect.height < 100) continue;
      
      const area = rect.width * rect.height;
      
      // If it takes up a significant portion of the screen (height > 40% of innerHeight)
      if (rect.height > window.innerHeight * 0.4 && area > maxArea) {
        // Check if the left sidebar is hidden, completely removed, or we are in a modal
        const paneSide = document.getElementById('pane-side');
        const isFullscreen = !paneSide || paneSide.getBoundingClientRect().width === 0;
        const hasStatusTestId = !!document.querySelector('[data-testid*="status"], [data-testid*="story"]');
        
        // Also check if it's a blob url (like the reference extension)
        const isBlob = m.src && m.src.includes('blob:https://web.whatsapp.com');
        
        // Check if any ancestor has high z-index
        let hasHighZIndex = false;
        let parent = m;
        while (parent && parent !== document.body) {
           const z = parseInt(window.getComputedStyle(parent).zIndex);
           if (z > 50) { hasHighZIndex = true; break; }
           parent = parent.parentElement;
        }
        
        if (isFullscreen || hasStatusTestId || hasHighZIndex || isBlob) {
          maxArea = area;
          targetMedia = m;
          isStatus = true;
        }
      }
    }

    const currentSrc = targetMedia ? (targetMedia.src || (targetMedia.querySelector('source') && targetMedia.querySelector('source').src) || targetMedia.currentSrc) : null;
    
    if (isStatus && targetMedia && currentSrc) {
      activeStatusUrl = currentSrc;
      btn.style.setProperty('display', 'block', 'important');
      btn.onclick = () => downloadMedia(currentSrc);
    } else {
      activeStatusUrl = null;
      btn.style.setProperty('display', 'none', 'important');
    }
  }, 1000);

  // Tambahkan shortcut keyboard dengan useCapture = true agar tidak diblokir WhatsApp
  document.addEventListener('keydown', (e) => {
    // Jika tombol Alt + S ditekan
    if (e.altKey && e.key.toLowerCase() === 's') {
      if (activeStatusUrl) {
        e.preventDefault(); // Mencegah fungsi browser default
        e.stopPropagation();
        console.log("Shortcut Alt+S ditekan! Menyimpan status...");
        downloadMedia(activeStatusUrl);
      } else {
        window?.ipc?.notifyEv?.({title: "WhatsZan", body: "Gagal: Tidak ada status yang terdeteksi di layar."});
      }
    }
  }, true);

  async function downloadMedia(url) {
    if (!url) return;
    try {
      const response = await fetch(url);
      const blob = await response.blob();
      const blobUrl = URL.createObjectURL(blob);
      
      const a = document.createElement('a');
      a.href = blobUrl;
      const isVideo = blob.type.includes('video') || url.includes('.mp4');
      a.download = `Status_WhatsApp_${Date.now()}.${isVideo ? 'mp4' : 'jpg'}`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(blobUrl);
      
      const originalText = btn.innerHTML;
      btn.innerHTML = 'Tersimpan!';
      setTimeout(() => btn.innerHTML = originalText, 2000);
      window?.ipc?.notifyEv?.({title: "WhatsZan", body: `Status berhasil disimpan ke folder Download!`});
    } catch (e) {
      console.error("Failed to download status:", e);
      btn.innerHTML = 'Gagal';
      setTimeout(() => btn.innerHTML = 'Simpan Status', 2000);
      window?.ipc?.notifyEv?.({title: "WhatsZan", body: "Gagal mengunduh status. Membuka di tab baru..."});
      
      // Fallback
      const a = document.createElement('a');
      a.href = url;
      a.target = '_blank';
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
    }
  }
}

function ewSetupGhostMode() {
  console.log("Ghost Mode Webpack hook starting...");
  
  // WhatsApp uses window.webpackChunkwhatsapp_web_client
  let wpRequire;
  try {
    if (!window.webpackChunkwhatsapp_web_client) return;
    
    window.webpackChunkwhatsapp_web_client.push([
      [Symbol("ghost_mode")],
      {},
      (req) => { wpRequire = req; }
    ]);
  } catch (e) {
    console.error("Failed to extract wpRequire", e);
    return;
  }
  
  if (!wpRequire || !wpRequire.m) return;
  
  const blockFunction = (origFunc, name) => {
    return function(...args) {
      if (name === 'Read Receipt' && window.wzIncognito?.noRead) {
        console.log(`[Ghost Mode] Blocked ${name}`);
        return Promise.resolve();
      }
      if (name === 'Typing Indicator' && window.wzIncognito?.noTyping) {
        console.log(`[Ghost Mode] Blocked ${name}`);
        return Promise.resolve();
      }
      return origFunc.apply(this, args);
    };
  };

  const patchModule = (mod, searchStr, name) => {
    if (!mod) return;
    try {
      // Direct exports
      for (const key in mod) {
        if (typeof mod[key] === 'function' && mod[key].toString().includes(searchStr)) {
          mod[key] = blockFunction(mod[key], name);
          return true;
        }
      }
      // Webpack 5 getters
      const descriptors = Object.getOwnPropertyDescriptors(mod);
      for (const key in descriptors) {
        const desc = descriptors[key];
        if (desc.get) {
          try {
            const origExport = desc.get();
            if (typeof origExport === 'function' && origExport.toString().includes(searchStr)) {
               Object.defineProperty(mod, key, {
                 get: () => blockFunction(origExport, name),
                 configurable: true
               });
               return true;
            } else if (typeof origExport === 'object' && origExport !== null) {
               for (const subKey in origExport) {
                 if (typeof origExport[subKey] === 'function' && origExport[subKey].toString().includes(searchStr)) {
                    origExport[subKey] = blockFunction(origExport[subKey], name);
                    return true;
                 }
               }
            }
          } catch(e) {}
        }
      }
    } catch(e) {}
    return false;
  };

  for (const modId in wpRequire.m) {
    const modStr = wpRequire.m[modId].toString();
    
    // Disable Read Receipts (Blue Ticks)
    if (modStr.includes('sendReadReceipt')) {
      const mod = wpRequire(modId);
      if (patchModule(mod, 'sendReadReceipt', 'Read Receipt')) {
         console.log(`[Ghost Mode] Successfully patched Read Receipts in module ${modId}`);
      }
    }
    
    // Disable Typing Indicator
    if (modStr.includes('sendChatstate')) {
      const mod = wpRequire(modId);
      if (patchModule(mod, 'sendChatstate', 'Typing Indicator')) {
         console.log(`[Ghost Mode] Successfully patched Typing Indicator in module ${modId}`);
      }
    }
  }
}

async function ewSetup() {
  console.log("ewSetup");
  ewHijackClick();

  const prefix = await window?.ipc?.stateGet?.("notifPrefix");
  ewHijackNotif(prefix);

  await ewSetupKeys();
  await ewSetupDictionary();
  const useAntiDelete = await window?.ipc?.stateGet?.("incognito-antidelete");
  const useStatusSaver = await window?.ipc?.stateGet?.("incognito-statussaver");
  
  // Set initial window.wzIncognito state
  window.wzIncognito = {
    noRead: await window?.ipc?.stateGet?.("incognito-noread"),
    noTyping: await window?.ipc?.stateGet?.("incognito-notyping"),
    antiDelete: await window?.ipc?.stateGet?.("incognito-antidelete")
  };
  
  ewSetupAntiDelete();
  if (useStatusSaver) ewSetupStatusSaver();
  ewSetupGhostMode();
  
  ewReplaceLogo();
  ewHijackTitle();

  addEventListener(
    "keydown",
    (ev) => {
      if (ev.key === "Escape") {
        const chatOpen = !!document.getElementById("main");
        if (!chatOpen) {
          ev.preventDefault();
          ev.stopPropagation();
          window?.ipc?.escapePressed?.();
        }
      }
    },
    true
  );
}

void ewSetup();

function ewCloseChat() {
  ewDoWhatsappAction("CLOSE_CHAT");
}
