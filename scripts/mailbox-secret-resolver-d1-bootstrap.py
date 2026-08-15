#!/usr/bin/env python3
"""Fail-closed one-time bootstrap for a brand-new mailbox resolver D1."""
from __future__ import annotations

import argparse, hashlib, importlib.util, json, re, sqlite3, tempfile
from pathlib import Path
from typing import Any

ROOT=Path(__file__).resolve().parents[1]
MIGRATIONS=Path("migrations/resolver-d1")
RELEASE_TOOL=Path("scripts/mailbox-secret-resolver-release.py")
DEFAULT_OUTPUT=ROOT/"artifacts/mailbox-secret-resolver-d1-first-bootstrap/bootstrap.sql"
MIGRATION_RE=re.compile(r"^(?P<n>[0-9]{4})_[a-z0-9_]+\.sql$")
COMMIT_RE=re.compile(r"^[0-9a-f]{40}$"); SHA_RE=re.compile(r"^[0-9a-f]{64}$")
WRANGLER_VERSION="4.94.0"; LEDGER="d1_migrations"
LEDGER_SQL='CREATE TABLE "d1_migrations" (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT UNIQUE, applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP NOT NULL);'
FRESH_ROW={"type":"table","name":"_cf_KV","tbl_name":"_cf_KV"}
EVIDENCE_KEYS={"schema_version","evidence_kind","resolver_release_id","resolver_source_commit_sha","resolver_migration_manifest_sha256","wrangler_version","fresh_target_schema","bootstrap","migration_inventory","schema_inventory","first_import","migration_ledger","replay","user_data_involved","secret_material_recorded","production_touched"}

class BootstrapError(ValueError): pass
def fail(m:str)->None: raise BootstrapError(m)
def canon(v:Any)->bytes: return json.dumps(v,sort_keys=True,separators=(",",":"),ensure_ascii=False).encode()
def sha(v:bytes)->str: return hashlib.sha256(v).hexdigest()

def validated_migrations(d:Path)->list[Path]:
    if d.is_symlink() or not d.is_dir(): fail(f"migration root must be a real directory: {d}")
    es=sorted(d.iterdir(),key=lambda p:p.name)
    if not es: fail("resolver migration inventory must not be empty")
    out=[]; nums=[]
    for p in es:
        if p.is_symlink() or not p.is_file(): fail(f"migration must be regular: {p.name}")
        m=MIGRATION_RE.fullmatch(p.name)
        if not m: fail(f"unexpected migration filename: {p.name}")
        b=p.read_bytes()
        if b"\0" in b: fail(f"migration contains NUL: {p.name}")
        try: t=b.decode("utf-8")
        except UnicodeDecodeError as e: raise BootstrapError(f"migration is not UTF-8: {p.name}") from e
        if not t.strip(): fail(f"migration is empty: {p.name}")
        out.append(p); nums.append(int(m.group("n")))
    if len(nums)!=len(set(nums)): fail("duplicate resolver migration numeric prefix")
    if nums!=list(range(1,len(out)+1)): fail(f"resolver migrations must be contiguous from 0001: {nums}")
    return out

def _inventory(d:Path)->list[dict[str,Any]]:
    return [{"path":p.name,"size":p.stat().st_size,"sha256":sha(p.read_bytes())} for p in validated_migrations(d)]
def migration_manifest_sha256(d:Path)->str: return sha(canon(_inventory(d)))

def _guard()->bytes:
    return b"""SELECT CASE WHEN EXISTS (SELECT 1 FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' AND NOT (type='table' AND name='_cf_KV' AND tbl_name='_cf_KV')) THEN abs(-9223372036854775808) ELSE 1 END AS resolver_empty_d1_guard;\n"""
def build_bootstrap_bytes(d:Path)->bytes:
    parts=[_guard(),LEDGER_SQL.encode()+b"\n"]
    for p in validated_migrations(d):
        b=p.read_bytes(); parts.append(b); parts.append(b"" if b.endswith(b"\n") else b"\n")
        parts.append(f'INSERT INTO "{LEDGER}" (name) VALUES (\'{p.name}\');\n'.encode())
    return b"\n".join(parts)
def write_bootstrap(d:Path,out:Path)->tuple[int,str]:
    b=build_bootstrap_bytes(d); out.parent.mkdir(parents=True,exist_ok=True); out.write_bytes(b); return len(b),sha(b)

def _schema_rows(c:sqlite3.Connection)->list[dict[str,str]]:
    rs=c.execute("SELECT type,name,tbl_name,COALESCE(sql,'') FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' AND name NOT IN (?, '_cf_KV') ORDER BY type,name,tbl_name,COALESCE(sql,'')",(LEDGER,)).fetchall()
    return [{"type":str(r[0]),"name":str(r[1]),"tbl_name":str(r[2]),"sql":str(r[3])} for r in rs]
def schema_inventory_metadata(d:Path)->dict[str,Any]:
    c=sqlite3.connect(":memory:")
    try:
        c.execute("PRAGMA foreign_keys=ON")
        for p in validated_migrations(d): c.executescript(p.read_text(encoding="utf-8"))
        rows=_schema_rows(c)
    finally: c.close()
    if not rows: fail("resolver migrations produced no application schema")
    return {"count":len(rows),"sha256":sha(canon(rows))}
def _ledger_names(c:sqlite3.Connection)->list[str]: return [str(r[0]) for r in c.execute(f'SELECT name FROM "{LEDGER}" ORDER BY id')]
def _check_ledger(c:sqlite3.Connection)->None:
    r=c.execute("SELECT sql FROM sqlite_master WHERE type='table' AND name=?",(LEDGER,)).fetchone()
    if not r or r[0]!=LEDGER_SQL.rstrip(";"): fail("canonical d1_migrations schema drifted")
    cols=[(r[1],r[2],r[3],r[4],r[5]) for r in c.execute(f'PRAGMA table_info("{LEDGER}")')]
    if cols!=[("id","INTEGER",0,None,1),("name","TEXT",0,None,0),("applied_at","TIMESTAMP",1,"CURRENT_TIMESTAMP",0)]: fail("canonical d1_migrations columns drifted")
    if ["name"] not in [[str(x[2]) for x in c.execute(f'PRAGMA index_info("{r[1]}")')] for r in c.execute(f'PRAGMA index_list("{LEDGER}")') if int(r[2])==1]: fail("canonical ledger name uniqueness missing")
def prove_bootstrap_parity(d:Path)->None:
    ps=validated_migrations(d); a=sqlite3.connect(":memory:"); b=sqlite3.connect(":memory:")
    try:
        for c in (a,b): c.execute("PRAGMA foreign_keys=ON")
        for p in ps: a.executescript(p.read_text(encoding="utf-8"))
        b.executescript(build_bootstrap_bytes(d).decode())
        if _schema_rows(a)!=_schema_rows(b): fail("bootstrap schema differs from sequential migrations")
        _check_ledger(b)
        if _ledger_names(b)!=[p.name for p in ps]: fail("bootstrap ledger differs from migration inventory")
    finally: a.close(); b.close()

def validate_empty_query_document(v:Any)->None:
    if not isinstance(v,list) or len(v)!=1 or not isinstance(v[0],dict) or v[0].get("success") is not True or not isinstance(v[0].get("results"),list): fail("fresh-target Wrangler JSON shape invalid")
    if v[0]["results"]!=[FRESH_ROW]: fail(f"target is not exact fresh resolver D1: {v[0]['results']}")
def validate_empty_query_file(p:Path)->None:
    try: v=json.loads(p.read_text(encoding="utf-8"))
    except (OSError,UnicodeDecodeError,json.JSONDecodeError) as e: raise BootstrapError(f"cannot read fresh-target JSON: {e}") from e
    validate_empty_query_document(v)

def _exact(v:Any,ks:set[str],label:str)->dict[str,Any]:
    if not isinstance(v,dict) or set(v)!=ks: fail(f"{label} exact-key inventory mismatch")
    return v
def _pos(v:Any,label:str)->int:
    if not isinstance(v,int) or isinstance(v,bool) or v<=0: fail(f"{label} must be positive integer")
    return v
def _sha(v:Any,label:str)->str:
    if not isinstance(v,str) or not SHA_RE.fullmatch(v): fail(f"{label} must be lowercase SHA-256")
    return v

def _expected(d:Path,ident:dict[str,str])->dict[str,Any]:
    b=build_bootstrap_bytes(d); names=[p.name for p in validated_migrations(d)]
    return {**ident,"bootstrap":{"bytes":len(b),"sha256":sha(b)},"migration_inventory":{"ordered_names":names,"count":len(names)},"schema_inventory":schema_inventory_metadata(d),"ledger_sha256":sha(canon(names))}
def verify_evidence_document(v:Any,d:Path,ident:dict[str,str])->None:
    e=_exact(v,EVIDENCE_KEYS,"evidence"); x=_expected(d,ident)
    if e["schema_version"]!=1 or e["evidence_kind"]!="resolver-d1-empty-bootstrap-remote-proof": fail("evidence schema/kind invalid")
    for k in ("resolver_release_id","resolver_source_commit_sha","resolver_migration_manifest_sha256"):
        if e[k]!=x[k]: fail(f"release identity mismatch: {k}")
    if not COMMIT_RE.fullmatch(str(e["resolver_source_commit_sha"])): fail("resolver source SHA malformed")
    _sha(e["resolver_migration_manifest_sha256"],"migration manifest")
    if e["wrangler_version"]!=WRANGLER_VERSION or e["fresh_target_schema"]!=[FRESH_ROW]: fail("evidence toolchain/fresh-target mismatch")
    if _exact(e["bootstrap"],{"bytes","sha256"},"bootstrap")!=x["bootstrap"]: fail("bootstrap identity mismatch")
    if _exact(e["migration_inventory"],{"ordered_names","count"},"migration inventory")!=x["migration_inventory"]: fail("migration inventory mismatch")
    if _exact(e["schema_inventory"],{"count","sha256"},"schema inventory")!=x["schema_inventory"]: fail("schema inventory mismatch")
    imp=_exact(e["first_import"],{"completed","statement_count","rows_written"},"first import")
    if imp["completed"] is not True: fail("first import incomplete")
    _pos(imp["statement_count"],"statement_count"); _pos(imp["rows_written"],"rows_written")
    names=x["migration_inventory"]["ordered_names"]
    if _exact(e["migration_ledger"],{"ordered_names","count","latest"},"migration ledger")!={"ordered_names":names,"count":len(names),"latest":names[-1]}: fail("migration ledger mismatch")
    r=_exact(e["replay"],{"rejected","error_class","schema_count_before","schema_count_after","schema_sha256_before","schema_sha256_after","ordered_names_before","ordered_names_after","ledger_sha256_before","ledger_sha256_after","residue"},"replay")
    if r["rejected"] is not True or not isinstance(r["error_class"],str) or not r["error_class"]: fail("replay not rejected")
    _pos(r["schema_count_before"],"schema_count_before"); _pos(r["schema_count_after"],"schema_count_after")
    for k in ("schema_sha256_before","schema_sha256_after","ledger_sha256_before","ledger_sha256_after"): _sha(r[k],k)
    if r["schema_count_before"]!=r["schema_count_after"] or r["schema_sha256_before"]!=r["schema_sha256_after"]: fail("replay changed schema")
    if r["ordered_names_before"]!=names or r["ordered_names_after"]!=names or r["ledger_sha256_before"]!=x["ledger_sha256"] or r["ledger_sha256_after"]!=x["ledger_sha256"]: fail("replay changed ledger")
    if r["residue"]!=[]: fail("replay left residue")
    for k in ("user_data_involved","secret_material_recorded","production_touched"):
        if e[k] is not False: fail(f"evidence must keep {k}=false")
def verify_evidence_file(p:Path,d:Path,ident:dict[str,str])->None:
    try: v=json.loads(p.read_text(encoding="utf-8"))
    except (OSError,UnicodeDecodeError,json.JSONDecodeError) as e: raise BootstrapError(f"cannot read evidence JSON: {e}") from e
    verify_evidence_document(v,d,ident)

def _release_tool()->Any:
    spec=importlib.util.spec_from_file_location("resolver_release",ROOT/RELEASE_TOOL)
    if not spec or not spec.loader: fail("cannot load resolver release verifier")
    m=importlib.util.module_from_spec(spec); spec.loader.exec_module(m); return m
def verified_release_input(archive:Path,source:str)->tuple[Any,Path,dict[str,str]]:
    if not COMMIT_RE.fullmatch(source): fail("expected source must be exact lowercase 40-hex")
    rel=_release_tool(); tmp=tempfile.TemporaryDirectory(prefix="resolver-bootstrap-release-")
    try:
        rd=rel.safe_extract(archive,Path(tmp.name)); manifest=rel.verify_directory(ROOT,rd,source); md=rd/MIGRATIONS; digest=migration_manifest_sha256(md)
        if digest!=manifest.get("resolver_migration_manifest_sha256"): fail("immutable release is not first-bootstrap eligible")
        return tmp,md,{"resolver_release_id":str(manifest["release_id"]),"resolver_source_commit_sha":str(manifest["source_commit_sha"]),"resolver_migration_manifest_sha256":digest}
    except Exception: tmp.cleanup(); raise

def check_repository_policy(root:Path=ROOT)->None:
    d=root/MIGRATIONS; a=build_bootstrap_bytes(d); b=build_bootstrap_bytes(d)
    if a!=b: fail("bootstrap SQL is not deterministic")
    prove_bootstrap_parity(d); meta=schema_inventory_metadata(d)
    print(f"Resolver D1 first-bootstrap policy passed: migrations={len(validated_migrations(d))} bytes={len(a)} sha256={sha(a)} schema_objects={meta['count']}.")
def _reject(label:str,fn:Any)->None:
    try: fn()
    except (BootstrapError,sqlite3.DatabaseError,UnicodeDecodeError): return
    fail(f"negative fixture unexpectedly passed: {label}")
def _fixture_evidence(d:Path,ident:dict[str,str])->dict[str,Any]:
    x=_expected(d,ident); names=x["migration_inventory"]["ordered_names"]; s=x["schema_inventory"]
    return {"schema_version":1,"evidence_kind":"resolver-d1-empty-bootstrap-remote-proof",**ident,"wrangler_version":WRANGLER_VERSION,"fresh_target_schema":[FRESH_ROW.copy()],"bootstrap":x["bootstrap"],"migration_inventory":x["migration_inventory"],"schema_inventory":s,"first_import":{"completed":True,"statement_count":4,"rows_written":1},"migration_ledger":{"ordered_names":names,"count":len(names),"latest":names[-1]},"replay":{"rejected":True,"error_class":"SQLITE_ERROR","schema_count_before":s["count"],"schema_count_after":s["count"],"schema_sha256_before":s["sha256"],"schema_sha256_after":s["sha256"],"ordered_names_before":names,"ordered_names_after":names,"ledger_sha256_before":x["ledger_sha256"],"ledger_sha256_after":x["ledger_sha256"],"residue":[]},"user_data_involved":False,"secret_material_recorded":False,"production_touched":False}
def self_test()->None:
    with tempfile.TemporaryDirectory(prefix="resolver-bootstrap-selftest-") as t:
        d=Path(t)/"m"; d.mkdir(); p=d/"0001_fixture.sql"; original=b"CREATE TABLE resolver_fixture(id INTEGER PRIMARY KEY,value TEXT NOT NULL) STRICT;\nCREATE INDEX resolver_fixture_value ON resolver_fixture(value);\n"; p.write_bytes(original)
        prove_bootstrap_parity(d); payload=build_bootstrap_bytes(d)
        if payload!=build_bootstrap_bytes(d): fail("determinism failed")
        c=sqlite3.connect(":memory:")
        try:
            c.execute('CREATE TABLE "_cf_KV" (key TEXT PRIMARY KEY,value BLOB) WITHOUT ROWID'); c.executescript(payload.decode()); sr=_schema_rows(c); ln=_ledger_names(c); _reject("replay",lambda:c.executescript(payload.decode()))
            if sr!=_schema_rows(c) or ln!=_ledger_names(c): fail("replay changed state")
        finally: c.close()
        validate_empty_query_document([{"success":True,"results":[FRESH_ROW.copy()]}]); _reject("nonfresh",lambda:validate_empty_query_document([{"success":True,"results":[FRESH_ROW.copy(),{"type":"table","name":"x","tbl_name":"x"}]}]))
        p.rename(d/"0002_fixture.sql"); _reject("missing 0001",lambda:validated_migrations(d)); (d/"0002_fixture.sql").rename(p)
        q=d/"0002_q.sql"; q.write_text("SELECT 1;\n"); r=d/"0003_r.sql"; r.write_text("SELECT 1;\n"); q.rename(d/"0004_q.sql"); _reject("gap",lambda:validated_migrations(d)); (d/"0004_q.sql").rename(q); dup=d/"0002_dup.sql"; dup.write_text("SELECT 1;\n"); _reject("duplicate",lambda:validated_migrations(d)); dup.unlink(); q.unlink(); r.unlink()
        bad=d/"README.md"; bad.write_text("x"); _reject("unexpected",lambda:validated_migrations(d)); bad.unlink(); p.write_bytes(original+b"\0"); _reject("nul",lambda:validated_migrations(d)); p.write_bytes(b"\xff"); _reject("utf8",lambda:validated_migrations(d)); p.write_bytes(original)
        link=d/"0002_link.sql"
        try: link.symlink_to(p.name)
        except (OSError,NotImplementedError): pass
        else: _reject("symlink",lambda:validated_migrations(d)); link.unlink()
        ident={"resolver_release_id":"mailbox-secret-resolver-v1-sha256-"+"a"*64,"resolver_source_commit_sha":"b"*40,"resolver_migration_manifest_sha256":migration_manifest_sha256(d)}; e=_fixture_evidence(d,ident); verify_evidence_document(e,d,ident)
        for label,path,val in (("bootstrap","bootstrap.sha256","0"*64),("source","resolver_source_commit_sha","c"*40),("ledger","migration_ledger.ordered_names",[]),("replay","replay.schema_sha256_after","1"*64),("production","production_touched",True)):
            x=json.loads(json.dumps(e)); cur=x; parts=path.split(".");
            for k in parts[:-1]: cur=cur[k]
            cur[parts[-1]]=val; _reject(label,lambda x=x:verify_evidence_document(x,d,ident))
        x=json.loads(json.dumps(e)); x["database_id"]="forbidden"; _reject("extra evidence field",lambda:verify_evidence_document(x,d,ident))
    print("Resolver D1 first-bootstrap positive and negative self-tests passed.")

def _input(args:argparse.Namespace)->tuple[Any|None,Path,dict[str,str]]:
    if args.development_working_tree:
        d=ROOT/MIGRATIONS; return None,d,{"resolver_release_id":"development-working-tree","resolver_source_commit_sha":"0"*40,"resolver_migration_manifest_sha256":migration_manifest_sha256(d)}
    if args.release_archive is None or args.expected_source_sha is None: fail("external bootstrap requires --release-archive and --expected-source-sha")
    return verified_release_input(args.release_archive,args.expected_source_sha)
def _add_input(p:argparse.ArgumentParser)->None:
    p.add_argument("--development-working-tree",action="store_true"); p.add_argument("--release-archive",type=Path); p.add_argument("--expected-source-sha")
def main()->int:
    p=argparse.ArgumentParser(description=__doc__); s=p.add_subparsers(dest="cmd",required=True); s.add_parser("check"); s.add_parser("self-test"); b=s.add_parser("build"); _add_input(b); b.add_argument("--output",type=Path,default=DEFAULT_OUTPUT); ve=s.add_parser("validate-empty"); ve.add_argument("--query-json",type=Path,required=True); ev=s.add_parser("verify-evidence"); _add_input(ev); ev.add_argument("--file",type=Path,required=True); a=p.parse_args()
    if a.cmd=="check": check_repository_policy(); return 0
    if a.cmd=="self-test": self_test(); return 0
    if a.cmd=="validate-empty": validate_empty_query_file(a.query_json); print("Remote resolver D1 is exact-fresh and eligible for first bootstrap import."); return 0
    tmp,d,ident=_input(a)
    try:
        if a.cmd=="build": n,h=write_bootstrap(d,a.output); print(f"Built resolver D1 first-bootstrap SQL: path={a.output} bytes={n} sha256={h} release={ident['resolver_release_id']}.")
        else: verify_evidence_file(a.file,d,ident); print("Verified metadata-only resolver D1 first-bootstrap evidence.")
    finally:
        if tmp is not None: tmp.cleanup()
    return 0
if __name__=="__main__":
    try: raise SystemExit(main())
    except BootstrapError as e: raise SystemExit(f"resolver D1 first-bootstrap rejected: {e}") from e
