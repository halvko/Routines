const KEY = 'routines.v1';

const task = (id, title, after = [], when = '') => ({ id, type: 'task', title, when, after: after.map((a) => ({ id: a })) });
const sub = (id, routineId, after = [], when = '') => ({ id, type: 'routine', routineId, when, after: after.map((a) => ({ id: a })) });

export function seed() {
  return {
    routines: {
      breakfast: {
        id: 'breakfast', name: 'Breakfast',
        steps: [
          task('coffee', 'Start the coffee'),
          task('food', 'Make food'),
          task('eat', 'Eat', ['coffee', 'food']),
          task('clean', 'Clean up', ['eat']),
        ],
      },
      morning: {
        id: 'morning', name: 'Morning',
        steps: [
          task('wake', 'Get out of bed'),
          task('water', 'Drink a glass of water', ['wake']),
          task('gym', 'Gym session', ['water'], 'gym'),
          task('shower', 'Shower', ['wake', 'gym']),
          task('teeth', 'Brush teeth', ['shower']),
          task('dress', 'Get dressed', ['teeth']),
          sub('bf', 'breakfast', ['dress']),
          task('meds', 'Take vitamins', ['bf']),
        ],
      },
      travel: {
        id: 'travel', name: 'Travel',
        steps: [
          task('checkin', 'Check in online', [], 'flying'),
          task('passport', 'Passport', [], 'abroad'),
          task('adapter', 'Power adapter', [], 'abroad'),
          task('clothes', 'Clothes'),
          task('toiletries', 'Toiletries', ['clothes']),
          task('charger', 'Phone charger'),
          task('close', 'Pack the bag', ['passport', 'adapter', 'toiletries', 'charger', 'checkin']),
          task('lights', 'Lights off, windows shut', ['close']),
          task('lock', 'Lock the door', ['lights']),
        ],
      },
    },
    runs: {},
    backlog: [],
    prefs: { todayMode: 'mixed' },
  };
}

export function normalize(db) {
  db.prefs = { todayMode: 'mixed', ...(db.prefs || {}) };
  db.backlog = db.backlog || [];
  // A run with no steps can't be ticked or finished; older versions could create one.
  for (const [id, run] of Object.entries(db.runs)) {
    if (!run.nodes?.length && !run.finishedAt) delete db.runs[id];
  }
  return db;
}

export function load() {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) {
      const db = JSON.parse(raw);
      if (db && db.routines && db.runs) return normalize(db);
    }
  } catch {}
  return seed();
}

export function save(db) {
  try { localStorage.setItem(KEY, JSON.stringify(db)); } catch {}
}
