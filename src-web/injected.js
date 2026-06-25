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
  setInterval(() => {
    // 1. Replace aria-label SVGs
    const svgs = document.querySelectorAll('svg[aria-label="WhatsApp"]');
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

    // 2. Replace exact text nodes in UI (avoiding chat messages)
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, null, false);
    let node;
    while (node = walker.nextNode()) {
      if (node.nodeValue === 'WhatsApp' || node.nodeValue === 'WhatsApp Web') {
        let parent = node.parentElement;
        let isMessage = false;
        while (parent) {
          if (parent.getAttribute('data-testid') === 'msg-container' || parent.getAttribute('role') === 'row' || parent.id === 'main') {
            isMessage = true;
            break;
          }
          parent = parent.parentElement;
        }
        if (!isMessage) {
          node.nodeValue = node.nodeValue.replace('WhatsApp', 'WhatsZan');
        }
      }
    }
  }, 2000);
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

async function ewSetup() {
  console.log("ewSetup");
  ewHijackClick();

  const prefix = await window?.ipc?.stateGet?.("notifPrefix");
  ewHijackNotif(prefix);

  await ewSetupKeys();
  await ewSetupDictionary();
  
  ewReplaceLogo();
  ewHijackTitle();

  if (await window?.ipc?.stateGet("escToggle")) {
    addEventListener(
      "keydown",
      (ev) => {
        if (ev.key === "Escape") {
          const chatOpen = document.getElementById("main");
          if (!chatOpen) {
            ev.preventDefault();
            ev.stopPropagation();
            console.log("esc: toggle window");
            window?.ipc?.windowToggle?.();
          }
        }
      },
      true,
    );
  }
}

void ewSetup();

function ewCloseChat() {
  ewDoWhatsappAction("CLOSE_CHAT");
}
