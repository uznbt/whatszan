console.log("injected.js");

function ewHijackNotif(prefix) {
  window.realNotification = window.Notification;

  const override = {
    construct(target, args) {
      const title = `${prefix}${args[0]}`;
      const opts = args[1] || {};
      
      const dummy = new EventTarget();
      dummy.close = () => {};

      const sendToMain = async () => {
         let iconDataUrl = null;
         if (opts.icon && opts.icon.startsWith('blob:')) {
            try {
              const res = await fetch(opts.icon);
              const blob = await res.blob();
              iconDataUrl = await new Promise(r => {
                 const reader = new FileReader();
                 reader.onloadend = () => r(reader.result);
                 reader.readAsDataURL(blob);
              });
            } catch(e) { console.error("Failed to convert blob icon", e); }
         } else {
            iconDataUrl = opts.icon;
         }
         
         const id = Date.now().toString() + Math.random();
         window.__notifs = window.__notifs || {};
         window.__notifs[id] = dummy;
         
         if (window?.ipc?.notifySend) {
           window.ipc.logToMain?.(`[WZ Notif] Routing notification to main process: ${title}`);
           window.ipc.notifySend(id, title, opts.body, iconDataUrl);
         } else {
           const thing = new target(title, opts);
           thing.addEventListener("click", (ev) => {
             dummy.dispatchEvent(new Event('click'));
             window?.ipc?.notifyEv?.(JSON.stringify(ev));
           });
         }
      };
      
      sendToMain();
      return dummy;
    },
  };

  window.Notification = new Proxy(window.realNotification, override);
  
  if (window?.ipc?.onNotifyClick) {
    window.ipc.onNotifyClick((id) => {
      const dummy = window.__notifs?.[id];
      if (dummy) dummy.dispatchEvent(new Event('click'));
    });
  }

  if (window?.ipc?.onNotifyReply) {
    window.ipc.onNotifyReply((id, reply) => {
      const dummy = window.__notifs?.[id];
      if (dummy) {
        dummy.dispatchEvent(new Event('click'));

        window?.ipc?.logToMain?.(`[WZ Notif] Reply received for: ${id}`);
        setTimeout(() => {
          const dataTransfer = new DataTransfer();
          dataTransfer.setData('text/plain', reply);
          const event = new ClipboardEvent('paste', {
            clipboardData: dataTransfer,
            bubbles: true
          });
          
          const composeBox = document.querySelector('#main div[contenteditable="true"]') || document.querySelector('div[contenteditable="true"][data-tab="10"]');
          if (composeBox) {
            window?.ipc?.logToMain?.(`[WZ Notif] Found compose box, pasting text`);
            composeBox.focus();
            composeBox.dispatchEvent(event);
            
            setTimeout(() => {
              composeBox.dispatchEvent(new Event('input', { bubbles: true }));
              
              setTimeout(() => {
                const sendIcon = document.querySelector('[data-icon="send"]');
                const sendBtn = sendIcon ? sendIcon.closest('button') : (document.querySelector('button[aria-label="Send"]') || document.querySelector('button[aria-label="Kirim"]'));
                if (sendBtn) {
                  window?.ipc?.logToMain?.(`[WZ Notif] Send button found. Clicking it.`);
                  sendBtn.click();
                } else {
                  window?.ipc?.logToMain?.(`[WZ Notif] Send button NOT found!`);
                }
              }, 150);
            }, 50);
          } else {
            window?.ipc?.logToMain?.(`[WZ Notif] Compose box NOT found!`);
          }
        }, 500);
      }
    });
  }
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
  const getAppName = () => window.wzAppName || 'WhatsZan';
  const getIsWhatsApp = () => getAppName().toLowerCase() === 'whatsapp';

  const processTextNode = (node) => {
    const txt = node.nodeValue || "";
    
    // Meta AI Killer (Text based)
    if (txt === 'Meta AI' || txt === 'Tanya Meta AI' || txt === 'Ask Meta AI') {
      let parent = node.parentElement;
      let isMessage = false;
      while (parent) {
        if (parent.getAttribute && (parent.getAttribute('data-testid') === 'msg-container' || parent.getAttribute('role') === 'row' || parent.id === 'main')) {
          isMessage = true;
          break;
        }
        parent = parent.parentElement;
      }
      if (!isMessage) {
        const btn = node.parentElement && node.parentElement.closest ? node.parentElement.closest('[role="button"], button') : null;
        if (btn) btn.style.display = 'none';
        else {
          let p = node.parentElement;
          for (let i = 0; i < 2; i++) {
            if (p && p.tagName !== 'BODY') {
              p.style.display = 'none';
              p = p.parentElement;
            }
          }
        }
      }
    }

    if (txt === 'WhatsApp' || txt === 'WhatsApp Web') {
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
        node.nodeValue = getAppName();
      }
    }
  };

  const processElement = (root) => {
    if (!getIsWhatsApp()) {
      // 1. Check for SVGs inside this element
      const svgs = root.querySelectorAll ? root.querySelectorAll('svg[aria-label="WhatsApp"]') : [];
      if (root.tagName === 'svg' && root.getAttribute('aria-label') === 'WhatsApp') svgs.push(root);
      
      svgs.forEach(svg => {
        const parent = svg.parentElement;
        if (parent && !parent.querySelector('.whatszan-logo')) {
          svg.style.display = 'none';
          const span = document.createElement('span');
          span.className = 'whatszan-logo';
          span.textContent = getAppName();
          span.style.fontWeight = 'bold';
          span.style.fontSize = '18px';
          span.style.color = 'inherit'; 
          parent.appendChild(span);
        }
      });
    }

    // 2. Banner Killer (ALWAYS RUNS)
    const text = root.innerText || "";
    const currentAppName = getAppName();
    if (((text.includes('Unduh WhatsApp untuk Windows') || text.includes(`Unduh ${currentAppName} untuk Windows`)) && text.includes('fitur ekstra')) || 
        text.includes('Dapatkan WhatsApp untuk Windows') || text.includes(`Dapatkan ${currentAppName} untuk Windows`)) {
      
      const textWalker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, null, false);
      let tNode;
      const targetNodes = [];
      while (tNode = textWalker.nextNode()) {
        const v = tNode.nodeValue || "";
        if (v.includes('Unduh WhatsApp untuk Windows') || v.includes(`Unduh ${currentAppName} untuk Windows`) ||
            v.includes('Dapatkan WhatsApp untuk Windows') || v.includes(`Dapatkan ${currentAppName} untuk Windows`) ||
            v.includes('fitur ekstra')) {
           targetNodes.push(tNode);
        }
      }
      
      targetNodes.forEach(t => {
        let p = t.parentElement;
        let hiddenModal = false;
        while (p && p.tagName !== 'BODY' && p.id !== 'app') {
          if (p.getAttribute('role') === 'dialog' || p.getAttribute('aria-modal') === 'true') {
             p.style.display = 'none';
             let backdrop = p.parentElement;
             while (backdrop && backdrop.tagName !== 'BODY' && backdrop.id !== 'app') {
               const style = window.getComputedStyle(backdrop);
               if (style.position === 'fixed' || style.position === 'absolute' || style.zIndex > 50) {
                 backdrop.style.display = 'none';
               }
               backdrop = backdrop.parentElement;
             }
             hiddenModal = true;
             break;
          }
          p = p.parentElement;
        }
        
        if (!hiddenModal) {
          let parent = t.parentElement;
          for (let i = 0; i < 6; i++) {
            if (parent && parent.tagName !== 'BODY' && parent.id !== 'app') {
              const rect = parent.getBoundingClientRect();
              if (rect.height > 10 && rect.height < 300) {
                parent.style.display = 'none';
              }
              parent = parent.parentElement;
            }
          }
        }
      });
    }

    // 2.5 Meta AI Killer
    const metaAiIcons = root.querySelectorAll ? Array.from(root.querySelectorAll('[title*="Meta AI"], [aria-label*="Meta AI"]')) : [];
    if (root.getAttribute && ((root.getAttribute('title') || '').includes('Meta AI') || (root.getAttribute('aria-label') || '').includes('Meta AI'))) {
      metaAiIcons.push(root);
    }
    metaAiIcons.forEach(icon => {
      const btn = icon.closest ? icon.closest('[role="button"], button') : null;
      if (btn) btn.style.display = 'none';
      else icon.style.display = 'none';
    });

    if (!getIsWhatsApp()) {
      // 3. Process all text nodes inside this element
      const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, null, false);
      let tNode;
      while (tNode = walker.nextNode()) {
        processTextNode(tNode);
      }
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
  // Ensure the document title also says customAppName instead of WhatsApp and remove " Web"
  const titleEl = document.querySelector('title');
  if (titleEl) {
    const updateTitle = () => {
      const getAppName = () => window.wzAppName || 'WhatsZan';
      let newTitle = document.title;
      if (newTitle.includes('WhatsApp')) {
        newTitle = newTitle.replace('WhatsApp', getAppName());
      }
      if (newTitle.endsWith(' Web')) {
        newTitle = newTitle.slice(0, -4);
      }
      if (document.title !== newTitle) {
        document.title = newTitle;
      }
    };
    new MutationObserver(updateTitle).observe(titleEl, { childList: true });
    // Trigger once initially
    updateTitle();
  }
}

async function ewSetupDictionary() {
  let triggerMap = {};
  let triggerList = [];
  let maxTriggerLen = 0;

  // Load dict and rebuild trigger map
  const loadDict = async () => {
    const dict = await window?.ipc?.stateGet?.("textReplacements");
    triggerMap = {};
    if (dict && Object.keys(dict).length > 0) {
      for (const [k, v] of Object.entries(dict)) {
        const key = k.startsWith('/') ? k : '/' + k;
        triggerMap[key.toLowerCase()] = v;
      }
    }
    triggerList = Object.keys(triggerMap);
    maxTriggerLen = triggerList.length > 0 ? Math.max(...triggerList.map(t => t.length)) : 0;
  };

  await loadDict();

  // Reload dict whenever settings are saved
  window?.ipc?.onSettingsSaved?.(() => loadDict());

  let buffer = '';
  let isReplacing = false;

  // Track backspace and reset keys for buffer management
  document.addEventListener('keydown', (ev) => {
    if (!ev.target.isContentEditable) { buffer = ''; return; }
    // window?.ipc?.logToMain?.(`[WZ Dict] keydown: ${ev.key}`);
    
    if (ev.key === 'Backspace') {
      buffer = buffer.slice(0, -1);
    } else if (ev.key === 'Enter' || ev.key === 'Escape' || ev.key === 'Tab') {
      buffer = '';
    } else if (ev.key.length === 1) {
      // Fallback: If WhatsApp swallows the 'input' event for special characters like '/',
      // we can at least know it was pressed. But we'll let 'input' handle it if it fires.
      // We will only manually add it to buffer if we detect it was swallowed. (We'll just log for now).
    }
  }, true);

  // Use input event (synchronous, has user gesture context)
  document.addEventListener('input', (ev) => {
    if (isReplacing) return;
    if (triggerList.length === 0) return;
    const target = ev.target;
    if (!target.isContentEditable) return;

    window?.ipc?.logToMain?.(`[WZ Dict] input event: type=${ev.inputType}, data=${ev.data}`);

    const selection = window.getSelection();
    if (!selection || !selection.focusNode) return;

    // Get the exact text in the current text node up to the cursor
    // This is 100% accurate and immune to swallowed events, mouse clicks, or backspaces!
    let textBeforeCursor = '';
    if (selection.focusNode.nodeType === Node.TEXT_NODE) {
      textBeforeCursor = selection.focusNode.textContent.slice(0, selection.focusOffset);
    } else if (selection.focusNode.innerText) {
      textBeforeCursor = selection.focusNode.innerText.slice(0, selection.focusOffset);
    }

    window?.ipc?.logToMain?.(`[WZ Dict] Text before cursor: "${textBeforeCursor}"`);

    const textLower = textBeforeCursor.toLowerCase();
    for (const trigger of triggerList) {
      if (textLower.endsWith(trigger)) {
        window?.ipc?.logToMain?.(`[WZ Dict] Trigger matched: ${trigger}`);
        isReplacing = true;

        const selection = window.getSelection();
        if (!selection || !selection.rangeCount) { isReplacing = false; break; }

        // Select the trigger text backwards
        for (let i = 0; i < trigger.length; i++) {
          selection.modify('extend', 'backward', 'character');
        }

        // Use Electron's native webContents.insertText
        const replacement = triggerMap[trigger];
        window?.ipc?.logToMain?.(`[WZ Dict] Sending to insertText: ${replacement}`);
        window?.ipc?.insertText?.(replacement);

        isReplacing = false;
        break;
      }
    }
  }, true);
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
  ewSetupJumpList();

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

function ewSetupJumpList() {
  const JUMP_LIST_THROTTLE_MS = 24 * 60 * 60 * 1000; // 24 hours

  window.addEventListener('blur', () => {
    try {
      const lastUpdate = parseInt(localStorage.getItem('wz_last_jump_list_update_v2') || '0', 10);
      const now = Date.now();
      if (now - lastUpdate < JUMP_LIST_THROTTLE_MS) return;

      // Mengambil setiap baris obrolan
      const listItems = document.querySelectorAll('#pane-side div[role="listitem"], #pane-side div[role="row"]');
      const recentChats = [];
      
      for (const item of listItems) {
        // Obrolan biasanya adalah span ber-title PERTAMA di dalam list item
        const titleEl = item.querySelector('span[title][dir="auto"], span[title]');
        if (titleEl) {
          let title = titleEl.getAttribute('title');
          if (title && title.trim() !== '') {
            // Bersihkan baris baru dan batasi panjang nama maksimal 50 karakter agar OS tidak menolak
            title = title.replace(/\r?\n|\r/g, ' ').trim();
            if (title.length > 50) title = title.substring(0, 47) + '...';

            if (!recentChats.includes(title)) {
              recentChats.push(title);
            }
          }
        }
        if (recentChats.length >= 5) break;
      }

      if (recentChats.length > 0) {
        window?.ipc?.updateRecentChats?.(recentChats);
        localStorage.setItem('wz_last_jump_list_update_v2', now.toString());
      }
    } catch (err) {
      console.error("Failed to update jump list", err);
    }
  });

  window?.ipc?.onJumpListAction?.((data) => {
    if (!data) return;
    
    // Attempt the action periodically until successful (e.g. if WA is still loading)
    let attempts = 0;
    const interval = setInterval(() => {
      attempts++;
      if (attempts > 10) clearInterval(interval);

      if (data.type === 'open-chat') {
        const searchInput = document.querySelector('#side div[contenteditable="true"]');
        if (searchInput) {
          clearInterval(interval);
          searchInput.focus();
          // Select all existing text if any to replace
          document.execCommand('selectAll', false, null);
          document.execCommand('insertText', false, data.name);
          
          setTimeout(() => {
            const results = document.querySelectorAll('#pane-side span[title]');
            for (const el of results) {
              if (el.title === data.name) {
                // WhatsApp list items are usually divs that listen to mousedown
                let clickable = el;
                while (clickable && clickable.tagName !== 'DIV') {
                  clickable = clickable.parentElement;
                }
                if (clickable) {
                  const mousedown = new MouseEvent('mousedown', { bubbles: true, cancelable: true, view: window });
                  clickable.dispatchEvent(mousedown);
                }
                break;
              }
            }
          }, 500);
        }
      } else if (data.type === 'action') {
        let clicked = false;
        if (data.name === 'new-chat') {
          // Find element containing "New chat" SVG path or similar
          const chatBtn = document.querySelector('div[title="New chat"], div[title="Chat baru"], span[data-icon="chat"], span[data-icon="new-chat-outline"]');
          if (chatBtn) {
            chatBtn.click();
            clicked = true;
          }
        } else if (data.name === 'new-call') {
          const callBtn = document.querySelector('div[title="Calls"], div[title="Panggilan"], span[data-icon="status-v3"]');
          if (callBtn) {
            callBtn.click();
            clicked = true;
          }
        }
        if (clicked) clearInterval(interval);
      }
    }, 1000);
  });
}

// --- File Sharing Feature ---
async function processPendingShare() {
  if (!window?.ipc?.getPendingShare) return;
  const share = await window.ipc.getPendingShare();
  if (!share || !share.files || share.files.length === 0) return;
  
  if (share.toastText) window.__wzToastText = share.toastText;
  
  window.ipc.logToMain?.(`[WZ Share] Received share request for ${share.files.length} files as ${share.type}`);
  
  const files = [];
  for (const path of share.files) {
    const data = await window.ipc.readFileBuffer(path);
    if (data && data.buffer) {
      const blob = new Blob([data.buffer]);
      let mimeType = 'application/octet-stream';
      const ext = data.name.split('.').pop().toLowerCase();
      if (['jpg', 'jpeg'].includes(ext)) mimeType = 'image/jpeg';
      else if (ext === 'png') mimeType = 'image/png';
      else if (ext === 'gif') mimeType = 'image/gif';
      else if (ext === 'webp') mimeType = 'image/webp';
      else if (ext === 'mp4') mimeType = 'video/mp4';
      else if (ext === 'pdf') mimeType = 'application/pdf';
      else if (ext === 'txt') mimeType = 'text/plain';
      else if (ext === 'csv') mimeType = 'text/csv';
      else if (ext === 'zip') mimeType = 'application/zip';
      
      const file = new File([blob], data.name, { type: mimeType });
      files.push(file);
    }
  }
  
  if (files.length === 0) return;
  
  uploadFiles(files, share.type === 'media');
}

function uploadFiles(files, asMedia) {
  const dt = new DataTransfer();
  files.forEach(f => dt.items.add(f));
  
  const injectToInput = () => {
    const inputs = document.querySelectorAll('input[type="file"]');
    let targetInput = null;
    if (asMedia) {
      targetInput = Array.from(inputs).find(i => i.accept && i.accept.includes('image/*'));
    } else {
      targetInput = Array.from(inputs).find(i => i.accept === '*');
    }
    
    if (targetInput) {
      targetInput.files = dt.files;
      targetInput.dispatchEvent(new Event('change', { bubbles: true }));
      window.ipc.logToMain?.(`[WZ Share] Successfully injected files into input`);
      return true;
    }
    return false;
  };
  
  // To ensure the global "Share with..." modal appears (Picture 1),
  // we must close the current chat if one is open.
  try {
    if (typeof ewCloseChat === 'function') ewCloseChat();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', keyCode: 27, bubbles: true }));
  } catch (err) {}
  
  setTimeout(() => {
    if (!injectToInput()) {
      window.ipc.logToMain?.(`[WZ Share] Global input not found. Opening New Chat sidebar as fallback...`);
      const chatBtn = document.querySelector('div[title="New chat"], div[title="Chat baru"], span[data-icon="chat"], span[data-icon="new-chat-outline"]');
      if (chatBtn) chatBtn.click();
      
      const pollInterval = setInterval(() => {
        if (injectToInput()) {
          clearInterval(pollInterval);
        }
      }, 500);
      setTimeout(() => clearInterval(pollInterval), 5000);
    }
  }, 200);
}

if (window?.ipc?.onShareFiles) {
  window.ipc.onShareFiles(() => {
    processPendingShare();
  });
  
  setInterval(() => {
    processPendingShare();
  }, 2000);
}
