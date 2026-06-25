#!/usr/bin/env python3
"""Load a Gryzzly catalog (exported via export-catalog.console.js) into the cockpit DB.

Keyless refresh: REPLACES the gryzzly_tasks catalog rows for the user with the export.
Usage: python3 import_catalog.py [catalog.json] [aggregated_plan.db] [user_id]
"""
import json, sqlite3, uuid, sys, os, datetime

JSON_PATH = sys.argv[1] if len(sys.argv) > 1 else os.path.expanduser('~/Téléchargements/gryzzly_catalog.json')
DB_PATH   = sys.argv[2] if len(sys.argv) > 2 else os.path.join(os.path.dirname(__file__), '..', '..', 'backend', 'aggregated_plan.db')
USER_ID   = sys.argv[3] if len(sys.argv) > 3 else '00000000-0000-0000-0000-000000000001'

with open(JSON_PATH, encoding='utf-8') as f:
    rows = json.load(f)   # each row: [gryzzly_task_id, name, gryzzly_project_id, project_name, customer_name, active(0/1)]

if not rows:
    sys.exit('Refusing to import an empty catalog — would wipe existing gryzzly_tasks. Aborting.')

now = datetime.datetime.now(datetime.timezone.utc).isoformat()
con = sqlite3.connect(DB_PATH)
cur = con.cursor()
try:
    cur.execute('DELETE FROM gryzzly_tasks WHERE user_id=?', (USER_ID,))
    n = 0
    for r in rows:
        gtid, name, gpid, pname, cust, active = r
        cur.execute('INSERT INTO gryzzly_tasks(id,user_id,gryzzly_task_id,name,gryzzly_project_id,project_name,customer_name,is_active,last_synced_at) VALUES(?,?,?,?,?,?,?,?,?)',
                    (str(uuid.uuid4()), USER_ID, gtid, name, gpid, pname, (cust or None), 1 if active else 0, now))
        n += 1
    con.commit()
except Exception as e:
    con.rollback()
    sys.exit(f'Import failed, rolled back: {e}')
active = sum(1 for r in rows if r[5])
print(f'Imported {n} Gryzzly tasks ({active} active) into {DB_PATH}')
con.close()
