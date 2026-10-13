// Shared GT/1 state in SQLite so several member processes see the same claims.
import { DatabaseSync } from "node:sqlite";
import fs from "node:fs";
import path from "node:path";

export function open(file) {
  const db = new DatabaseSync(file);
  db.exec(`PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;
    CREATE TABLE IF NOT EXISTS claims(resource TEXT PRIMARY KEY, holder TEXT, mode TEXT, until_ms INTEGER, reason TEXT, auto INTEGER);
    CREATE TABLE IF NOT EXISTS inbox(id INTEGER PRIMARY KEY, t INTEGER, sender TEXT, recipient TEXT, text TEXT, read INTEGER DEFAULT 0);
    CREATE TABLE IF NOT EXISTS log(t INTEGER, member TEXT, kind TEXT, data TEXT);`);
  return db;
}

export function norm(repo, r) {
  let p = String(r).replace(/^file:\/\//, "");
  const real = fs.realpathSync(repo);
  if (path.isAbsolute(p)) {
    const abs = fs.existsSync(p) ? fs.realpathSync(p) : path.resolve(fs.realpathSync(path.dirname(p)), path.basename(p));
    p = path.relative(real, abs);
  }
  return path.normalize(p).replace(/^(\.\/)+/, "");
}

export const log = (db, member, kind, data) =>
  db.prepare("INSERT INTO log VALUES (?,?,?,?)").run(Date.now(), member, kind, JSON.stringify(data));

// Returns the live claim on a resource, dropping expired ones.
export function holderOf(db, resource) {
  const c = db.prepare("SELECT * FROM claims WHERE resource=?").get(resource);
  if (c && c.until_ms && c.until_ms < Date.now()) {
    db.prepare("DELETE FROM claims WHERE resource=?").run(resource);
    return null;
  }
  return c || null;
}

export function claim(db, member, resource, { reason = "", auto = 0, ttl_ms = 0 } = {}) {
  db.exec("BEGIN IMMEDIATE");
  try {
    const c = holderOf(db, resource);
    if (c && c.holder !== member) { db.exec("COMMIT"); return { granted: false, resource, holder: c.holder, note: c.reason }; }
    const until = ttl_ms ? Date.now() + ttl_ms : 0;
    // An explicit claim upgrades an auto claim; an auto claim never downgrades an explicit one.
    if (c && c.holder === member && !c.auto && auto) {
      db.exec("COMMIT"); return { granted: true, resource, holder: member };
    }
    db.prepare("INSERT OR REPLACE INTO claims VALUES (?,?,?,?,?,?)").run(resource, member, "write", until, reason, auto);
    db.exec("COMMIT");
    return { granted: true, resource, holder: member };
  } catch (e) { db.exec("ROLLBACK"); throw e; }
}

export function release(db, member, resource) {
  db.prepare("DELETE FROM claims WHERE resource=? AND holder=?").run(resource, member);
}

// Hard gate used by the core for every proposed write.
export function gate(db, member, resource) {
  const r = claim(db, member, resource, { auto: 1, ttl_ms: 90000, reason: "auto claim from an edit" });
  log(db, member, r.granted ? "gate_allow" : "gate_deny", { resource, holder: r.holder });
  return r;
}
