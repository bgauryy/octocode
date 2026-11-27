const db = require('./db');

function findUser(userId) {
  return db.lookup(userId);
}

module.exports = { findUser };
