import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { consola } from "consola";
import crypto from "node:crypto";

const ENCRYPTION_KEY = crypto.scryptSync('whatszan-super-secret-key-2026', 'salt', 32);
const ALGORITHM = 'aes-256-gcm';

function encrypt(text) {
  const iv = crypto.randomBytes(16);
  const cipher = crypto.createCipheriv(ALGORITHM, ENCRYPTION_KEY, iv);
  let encrypted = cipher.update(text, 'utf8', 'hex');
  encrypted += cipher.final('hex');
  const authTag = cipher.getAuthTag().toString('hex');
  return `${iv.toString('hex')}:${authTag}:${encrypted}`;
}

function decrypt(text) {
  try {
    const parts = text.split(':');
    if (parts.length !== 3) return null;
    const iv = Buffer.from(parts[0], 'hex');
    const authTag = Buffer.from(parts[1], 'hex');
    const encrypted = parts[2];
    const decipher = crypto.createDecipheriv(ALGORITHM, ENCRYPTION_KEY, iv);
    decipher.setAuthTag(authTag);
    let decrypted = decipher.update(encrypted, 'hex', 'utf8');
    decrypted += decipher.final('utf8');
    return decrypted;
  } catch(e) {
    return null;
  }
}

export class JsonConfig {
  #filePath;
  #data;
  #parseError;

  constructor(filePath, defaultData = {}) {
    this.#filePath = filePath;
    this.#data = {};
    this.#parseError = null;
    this.#load(defaultData);
  }

  get file() {
    return this.#filePath;
  }

  get parseError() {
    return this.#parseError;
  }

  get data() {
    return this.#data;
  }

  get(key, defaultValue) {
    return Object.hasOwn(this.#data, key) ? this.#data[key] : defaultValue;
  }

  set(key, value) {
    this.#data[key] = value;
    this.#save();
  }

  delete(key) {
    delete this.#data[key];
    this.#save();
  }

  #load(defaultData) {
    if (!existsSync(this.#filePath)) {
      this.#data = { ...defaultData };
      return;
    }

    try {
      const rawData = readFileSync(this.#filePath, "utf-8");
      const decrypted = decrypt(rawData);
      if (decrypted) {
        this.#data = JSON.parse(decrypted);
      } else {
        // Fallback to raw JSON if it wasn't encrypted yet
        this.#data = JSON.parse(rawData);
      }
    } catch (err) {
      this.#parseError = err;
      consola.warn(`Config parse error in ${this.#filePath}:`, err.message);
      this.#data = { ...defaultData };
    }
  }

  #save() {
    try {
      const rawText = JSON.stringify(this.#data);
      const encrypted = encrypt(rawText);
      writeFileSync(this.#filePath, encrypted, "utf-8");
    } catch (err) {
      consola.error(`Failed to save config to ${this.#filePath}:`, err.message);
    }
  }
}
