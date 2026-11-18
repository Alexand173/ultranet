# Put one result on a PAST day in a disposable Records database, so that day can be closed now (a real registry
# day cannot be closed until it has finished). Uses the documented record-id hash. Disposable databases only.
import hashlib
import sqlite3
import sys

db, day, game, result_id = sys.argv[1:5]
connection = sqlite3.connect(db)
record_id_hash = hashlib.sha3_256(b"UltraRecord/record-id/v1" + result_id.encode()).digest()
connection.execute(
    "INSERT INTO registry_results (game_id, player, result_id, result_id_hash, score, data_hash, day, created_at)"
    " VALUES (?,?,?,?,?,?,?,?)",
    (game, "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e", result_id, record_id_hash, 1, bytes(32), day, 1),
)
connection.commit()
