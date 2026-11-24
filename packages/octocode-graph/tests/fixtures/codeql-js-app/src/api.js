const express = require('express');
const { findUser } = require('./service');

const app = express();

app.get('/users', (req, res) => {
  const id = req.query.id;
  findUser(id).then(rows => res.json(rows));
});

module.exports = app;
