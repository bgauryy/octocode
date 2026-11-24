const mysql = require('mysql');

const pool = mysql.createPool({ host: 'localhost' });

function lookup(id) {
  const sql = 'SELECT * FROM users WHERE id = ' + id;
  return new Promise((resolve, reject) =>
    pool.query(sql, (err, rows) => (err ? reject(err) : resolve(rows)))
  );
}

module.exports = { lookup };
