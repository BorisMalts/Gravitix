use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;
use std::cell::RefCell;
use crate::ast::{FnDef, BinOp};
use crate::error::{GravError, GravResult};

// ─────────────────────────────────────────────────────────────────────────────
// Structured bot output — replaces fragile "__to:" string protocol
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum BotOutput {
    /// Send a plain text message to a room
    Send { room_id: i64, text: String },
    /// Reply to a specific message
    Reply { room_id: i64, reply_to: i64, text: String },
    /// Send a message with inline keyboard
    /// buttons: rows of buttons, each button is (label, callback_data)
    Keyboard { room_id: i64, text: String, buttons: Vec<Vec<(String, String)>> },
    /// Answer an inline button callback query
    AnswerCallback { callback_id: String, text: Option<String> },
    /// Delete a message
    DeleteMessage { room_id: i64, msg_id: i64 },
    /// Send a rich structured message
    #[allow(dead_code)]
    RichMessage {
        room_id: i64,
        title:   Option<String>,
        text:    Option<String>,
        image:   Option<String>,
        buttons: Vec<Vec<(String, String)>>,
    },
    /// Send a message to a federated node  `federated emit "room@node" msg`
    #[allow(dead_code)]
    FederatedSend { target: String, text: String },
    /// Typing indicator (Feature 9)
    #[allow(dead_code)]
    Typing { room_id: i64 },
    /// Pin a message (Feature 9)
    #[allow(dead_code)]
    PinMsg { room_id: i64, msg_id: i64 },
    /// Unpin a message (Feature 9)
    #[allow(dead_code)]
    UnpinMsg { room_id: i64, msg_id: i64 },
    /// Mute a user (Feature 9)
    #[allow(dead_code)]
    MuteUser { room_id: i64, user_id: i64, duration_ms: Option<u64> },
    /// Ban a user from a room
    #[allow(dead_code)]
    BanUser { room_id: i64, user_id: i64, reason: Option<String> },
    /// Kick a user from a room
    #[allow(dead_code)]
    KickUser { room_id: i64, user_id: i64 },
    /// Set slow mode for a room (seconds, 0 to disable)
    #[allow(dead_code)]
    SetSlowMode { room_id: i64, seconds: u64 },
    /// Embed mini-app widget (Feature 7)
    #[allow(dead_code)]
    Embed { room_id: i64, html: Option<String>, url: Option<String>, height: i64, title: String },
    /// Push notification to a user (Feature 9 new)
    #[allow(dead_code)]
    Notify { user_id: i64, text: String },
    /// Push notification to a room (Feature 9 new)
    #[allow(dead_code)]
    NotifyRoom { room_id: i64, text: String },
    /// Interactive form (Feature W1)
    #[allow(dead_code)]
    Form { room_id: i64, fields: Vec<(String, String)>, submit: String },
    /// Interactive table (Feature W2)
    #[allow(dead_code)]
    Table { room_id: i64, text: String },
    /// Stream chunk (Feature W6)
    #[allow(dead_code)]
    StreamChunk { room_id: i64, text: String },
    /// Update Architex Mini App reactive state from Gravitix bot
    #[allow(dead_code)]
    UiUpdate { variable: String, value: String },
}

// ─────────────────────────────────────────────────────────────────────────────
// Runtime value  — designed to be cheap to clone:
//   primitives are Copy-like (wrapped in the enum),
//   heap values (Str, List, Map) use Rc so clone is O(1).
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    /// Complex number (re, im)
    Complex(f64, f64),
    Str(Rc<String>),
    List(Rc<RefCell<Vec<Value>>>),
    Map(Rc<RefCell<HashMap<String, Value>>>),
    Fn(Rc<FnDef>),

    /// The bot context injected in every handler  (chat_id, user info, raw message, …)
    Ctx(Rc<RefCell<BotCtx>>),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Null,    Value::Null)    => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a),  Value::Int(b))  => a == b,
            (Value::Float(a),Value::Float(b))=> a == b,
            (Value::Int(a),  Value::Float(b))=> (*a as f64) == *b,
            (Value::Float(a),Value::Int(b))  => *a == (*b as f64),
            (Value::Complex(ar, ai), Value::Complex(br, bi)) => ar == br && ai == bi,
            (Value::Complex(r, i), Value::Float(f)) | (Value::Float(f), Value::Complex(r, i)) => *i == 0.0 && r == f,
            (Value::Complex(r, i), Value::Int(n))   => *i == 0.0 && *r == (*n as f64),
            (Value::Int(n), Value::Complex(r, i))   => *i == 0.0 && (*n as f64) == *r,
            (Value::Str(a),  Value::Str(b))  => a == b,
            _ => false,
        }
    }
}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (Value::Int(a),   Value::Int(b))  => a.partial_cmp(b),
            (Value::Float(a), Value::Float(b))=> a.partial_cmp(b),
            (Value::Int(a),   Value::Float(b))=> (*a as f64).partial_cmp(b),
            (Value::Float(a), Value::Int(b))  => a.partial_cmp(&(*b as f64)),
            // Complex is only orderable when imaginary part is 0
            (Value::Complex(r, i), Value::Complex(r2, i2)) if *i == 0.0 && *i2 == 0.0 => r.partial_cmp(r2),
            (Value::Complex(r, i), Value::Float(f)) if *i == 0.0 => r.partial_cmp(f),
            (Value::Float(f), Value::Complex(r, i)) if *i == 0.0 => f.partial_cmp(r),
            (Value::Complex(r, i), Value::Int(n)) if *i == 0.0 => r.partial_cmp(&(*n as f64)),
            (Value::Int(n), Value::Complex(r, i)) if *i == 0.0 => (*n as f64).partial_cmp(r),
            (Value::Str(a),   Value::Str(b))  => a.as_str().partial_cmp(b.as_str()),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null        => write!(f, "null"),
            Value::Bool(b)     => write!(f, "{b}"),
            Value::Int(n)      => write!(f, "{n}"),
            Value::Float(x)    => write!(f, "{x}"),
            Value::Complex(re, im) => {
                if *re == 0.0 && *im == 0.0 {
                    write!(f, "0")
                } else if *re == 0.0 {
                    write!(f, "{im}i")
                } else if *im == 0.0 {
                    write!(f, "{re}")
                } else if *im < 0.0 {
                    write!(f, "{re}{im}i")
                } else {
                    write!(f, "{re}+{im}i")
                }
            }
            Value::Str(s)      => write!(f, "{s}"),
            Value::List(list)  => {
                let l = list.borrow();
                write!(f, "[")?;
                for (i, v) in l.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
            Value::Map(map) => {
                let m = map.borrow();
                write!(f, "{{")?;
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "\"{k}\": {v}")?;
                }
                write!(f, "}}")
            }
            Value::Fn(fd)  => write!(f, "<fn {}>", fd.name),
            Value::Ctx(_)  => write!(f, "<ctx>"),
        }
    }
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null       => "null",
            Value::Bool(_)    => "bool",
            Value::Int(_)     => "int",
            Value::Float(_)   => "float",
            Value::Complex(..)=> "complex",
            Value::Str(_)     => "str",
            Value::List(_)    => "list",
            Value::Map(_)     => "map",
            Value::Fn(_)      => "fn",
            Value::Ctx(_)     => "ctx",
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Null           => false,
            Value::Bool(b)        => *b,
            Value::Int(n)         => *n != 0,
            Value::Float(x)       => *x != 0.0,
            Value::Complex(r, i)  => *r != 0.0 || *i != 0.0,
            Value::Str(s)         => !s.is_empty(),
            Value::List(l)        => !l.borrow().is_empty(),
            Value::Map(m)         => !m.borrow().is_empty(),
            _                     => true,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        if let Value::Str(s) = self { Some(s.as_str()) } else { None }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(n)   => Some(*n),
            Value::Float(f) => Some(*f as i64),
            Value::Complex(r, i) if *i == 0.0 => Some(*r as i64),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Int(n)   => Some(*n as f64),
            Value::Float(f) => Some(*f),
            Value::Complex(r, i) if *i == 0.0 => Some(*r),
            _ => None,
        }
    }

    pub fn as_complex(&self) -> Option<(f64, f64)> {
        match self {
            Value::Complex(r, i) => Some((*r, *i)),
            Value::Int(n)        => Some((*n as f64, 0.0)),
            Value::Float(f)      => Some((*f, 0.0)),
            _ => None,
        }
    }

    pub fn make_complex(re: f64, im: f64) -> Self {
        Value::Complex(re, im)
    }

    pub fn make_str(s: impl Into<String>) -> Self {
        Value::Str(Rc::new(s.into()))
    }

    pub fn make_list(v: Vec<Value>) -> Self {
        Value::List(Rc::new(RefCell::new(v)))
    }

    pub fn make_map(m: HashMap<String, Value>) -> Self {
        Value::Map(Rc::new(RefCell::new(m)))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Bot context — available as `ctx` inside every handler
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BotCtx {
    /// Vortex room ID (replaces Telegram chat_id)
    pub room_id:       i64,
    /// Sender's user ID
    pub user_id:       i64,
    /// Sender's username
    pub username:      String,
    /// Message text (None for join/leave/callback events)
    pub text:          Option<String>,
    /// Message ID
    pub message_id:    i64,
    /// Slash command name without slash (e.g. "start" for /start), only set for Command updates
    pub command:       Option<String>,
    /// Command arguments (e.g. ["arg1", "arg2"] for /start arg1 arg2)
    pub args:          Vec<String>,
    /// Callback button data (only for Callback updates)
    pub callback_data: Option<String>,
    /// Callback query ID (only for Callback updates — needed for answer_callback)
    pub callback_id:   Option<String>,
    /// Unix timestamp of the event
    pub timestamp:     i64,
    /// Emoji reaction (only for Reaction updates)
    pub reaction:      Option<String>,
    /// File/image URL (for File, Image, VoiceMsg updates)
    pub file_url:      Option<String>,
    /// File size in bytes
    pub file_size:     Option<i64>,
    /// Voice message duration in seconds
    pub duration:      Option<i64>,
    /// Whether this is a DM (direct message)
    pub is_dm:         bool,
    /// Mention text (text after @mention, if present)
    pub mention_text:  Option<String>,
    /// Update type for routing
    pub update_kind:   UpdateKind,
    /// User's preferred language (Feature 12: i18n)
    #[allow(dead_code)]
    pub user_lang:     Option<String>,
    /// Webhook body (Feature 10)
    #[allow(dead_code)]
    pub webhook_body:    Option<Value>,
    /// Webhook headers (Feature 10)
    #[allow(dead_code)]
    pub webhook_headers: Option<Value>,
    /// Vote option (Feature 10: poll_vote trigger)
    #[allow(dead_code)]
    pub vote_option: Option<String>,
    /// Forward from user ID (Feature 10: forward trigger)
    #[allow(dead_code)]
    pub forward_from: Option<i64>,
    /// Whether this is a thread reply (Feature 10: thread trigger)
    #[allow(dead_code)]
    pub is_thread: bool,
    /// Detected intent name (Feature N1)
    #[allow(dead_code)]
    pub intent: Option<String>,
    /// Platform name (Feature N10)
    #[allow(dead_code)]
    pub platform: String,
}

/// The kind of update for routing (Feature 2 & 4)
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum UpdateKind {
    Message,
    Command,
    File,
    Image,
    VoiceMsg,
    Reaction,
    Join,
    Leave,
    Edited,
    Callback,
    Dm,
    Mention,
    Any,
    PollVote,
    Thread,
    Forward,
}

impl BotCtx {
    pub fn get_field(&self, field: &str) -> Value {
        match field {
            "room_id"                   => Value::Int(self.room_id),
            "user_id" | "id"            => Value::Int(self.user_id),
            "username" | "name"         => Value::make_str(&self.username),
            "text" | "msg_text"         => self.text.as_deref()
                                               .map(Value::make_str)
                                               .unwrap_or(Value::Null),
            "message_id" | "msg_id"     => Value::Int(self.message_id),
            "command"                   => self.command.as_deref()
                                               .map(Value::make_str)
                                               .unwrap_or(Value::Null),
            "args"                      => Value::make_list(
                                               self.args.iter()
                                                   .map(|a| Value::make_str(a.as_str()))
                                                   .collect()
                                           ),
            "callback_data" | "data"    => self.callback_data.as_deref()
                                               .map(Value::make_str)
                                               .unwrap_or(Value::Null),
            "callback_id"               => self.callback_id.as_deref()
                                               .map(Value::make_str)
                                               .unwrap_or(Value::Null),
            "timestamp"                 => Value::Int(self.timestamp),
            "reaction"                  => self.reaction.as_deref()
                                               .map(Value::make_str)
                                               .unwrap_or(Value::Null),
            "file_url"                  => self.file_url.as_deref()
                                               .map(Value::make_str)
                                               .unwrap_or(Value::Null),
            "file_size"                 => self.file_size
                                               .map(Value::Int)
                                               .unwrap_or(Value::Null),
            "duration"                  => self.duration
                                               .map(Value::Int)
                                               .unwrap_or(Value::Null),
            "is_dm"                     => Value::Bool(self.is_dm),
            "mention_text"              => self.mention_text.as_deref()
                                               .map(Value::make_str)
                                               .unwrap_or(Value::Null),
            "user_name"                 => Value::make_str(&self.username),
            "user_lang" | "lang"        => self.user_lang.as_deref()
                                               .map(Value::make_str)
                                               .unwrap_or_else(|| Value::make_str("en")),
            "webhook_body"              => self.webhook_body.clone().unwrap_or(Value::Null),
            "webhook_headers"           => self.webhook_headers.clone().unwrap_or(Value::Null),
            "vote_option"               => self.vote_option.as_deref()
                                               .map(Value::make_str)
                                               .unwrap_or(Value::Null),
            "forward_from"              => self.forward_from
                                               .map(Value::Int)
                                               .unwrap_or(Value::Null),
            "is_thread"                 => Value::Bool(self.is_thread),
            "intent"                    => self.intent.as_deref()
                                               .map(Value::make_str)
                                               .unwrap_or(Value::Null),
            "platform"                  => Value::make_str(&self.platform),
            _                           => Value::Null,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Binary operator application
// ─────────────────────────────────────────────────────────────────────────────

pub fn apply_binop(op: BinOp, lhs: Value, rhs: Value) -> GravResult<Value> {
    // Helper: promote operands to complex if either side is Complex
    fn try_complex(lhs: &Value, rhs: &Value) -> Option<((f64, f64), (f64, f64))> {
        match (lhs, rhs) {
            (Value::Complex(..), _) | (_, Value::Complex(..)) => {
                Some((lhs.as_complex()?, rhs.as_complex()?))
            }
            _ => None,
        }
    }

    match op {
        BinOp::Add => {
            if let Some(((ar, ai), (br, bi))) = try_complex(&lhs, &rhs) {
                return Ok(Value::Complex(ar + br, ai + bi));
            }
            match (&lhs, &rhs) {
                (Value::Int(a),   Value::Int(b))   => Ok(Value::Int(a + b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
                (Value::Int(a),   Value::Float(b)) => Ok(Value::Float(*a as f64 + b)),
                (Value::Float(a), Value::Int(b))   => Ok(Value::Float(a + *b as f64)),
                (Value::Str(a),   Value::Str(b))   => Ok(Value::make_str(format!("{a}{b}"))),
                (Value::Str(a),   _)               => Ok(Value::make_str(format!("{a}{rhs}"))),
                (_,               Value::Str(b))   => Ok(Value::make_str(format!("{lhs}{b}"))),
                _ => Err(GravError::Runtime(format!("cannot add {} and {}", lhs.type_name(), rhs.type_name()))),
            }
        },
        BinOp::Sub => {
            if let Some(((ar, ai), (br, bi))) = try_complex(&lhs, &rhs) {
                return Ok(Value::Complex(ar - br, ai - bi));
            }
            numeric_op(op, lhs, rhs)
        },
        BinOp::Mul => {
            // Complex: (a+bi)(c+di) = (ac-bd) + (ad+bc)i
            if let Some(((a, b), (c, d))) = try_complex(&lhs, &rhs) {
                return Ok(Value::Complex(a * c - b * d, a * d + b * c));
            }
            match (&lhs, &rhs) {
                (Value::Int(a),   Value::Int(b))   => Ok(Value::Int(a * b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
                (Value::Int(a),   Value::Float(b)) => Ok(Value::Float(*a as f64 * b)),
                (Value::Float(a), Value::Int(b))   => Ok(Value::Float(a * *b as f64)),
                (Value::Str(s),   Value::Int(n)) | (Value::Int(n), Value::Str(s)) =>
                    Ok(Value::make_str(s.as_str().repeat(*n as usize))),
                _ => Err(GravError::Runtime(format!("cannot multiply {} and {}", lhs.type_name(), rhs.type_name()))),
            }
        },
        BinOp::Div => {
            // Complex: (a+bi)/(c+di) = ((ac+bd) + (bc-ad)i) / (c^2+d^2)
            if let Some(((a, b), (c, d))) = try_complex(&lhs, &rhs) {
                let denom = c * c + d * d;
                if denom == 0.0 {
                    return Err(GravError::Runtime("complex division by zero".into()));
                }
                return Ok(Value::Complex((a * c + b * d) / denom, (b * c - a * d) / denom));
            }
            numeric_op(op, lhs, rhs)
        },
        BinOp::Rem => numeric_op(op, lhs, rhs),
        BinOp::Pow => {
            // Complex power using polar form: z^w = exp(w * ln(z))
            if let Some(((a, b), (c, d))) = try_complex(&lhs, &rhs) {
                let r = (a * a + b * b).sqrt();
                if r == 0.0 {
                    return Ok(Value::Complex(0.0, 0.0));
                }
                let theta = b.atan2(a);
                let ln_r = r.ln();
                // w * ln(z) = (c+di)(ln_r + theta*i)
                //            = (c*ln_r - d*theta) + (d*ln_r + c*theta)i
                let exp_re = c * ln_r - d * theta;
                let exp_im = d * ln_r + c * theta;
                let mag = exp_re.exp();
                return Ok(Value::Complex(mag * exp_im.cos(), mag * exp_im.sin()));
            }
            match (&lhs, &rhs) {
                (Value::Int(a),   Value::Int(b))   => Ok(Value::Int(a.pow(*b as u32))),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a.powf(*b))),
                (Value::Int(a),   Value::Float(b)) => Ok(Value::Float((*a as f64).powf(*b))),
                (Value::Float(a), Value::Int(b))   => Ok(Value::Float(a.powi(*b as i32))),
                _ => Err(GravError::Runtime(format!("cannot pow {} and {}", lhs.type_name(), rhs.type_name()))),
            }
        },
        BinOp::Eq  => Ok(Value::Bool(lhs == rhs)),
        BinOp::Ne  => Ok(Value::Bool(lhs != rhs)),
        BinOp::Lt  => Ok(Value::Bool(lhs < rhs)),
        BinOp::Gt  => Ok(Value::Bool(lhs > rhs)),
        BinOp::Le  => Ok(Value::Bool(lhs <= rhs)),
        BinOp::Ge  => Ok(Value::Bool(lhs >= rhs)),
        BinOp::And => Ok(Value::Bool(lhs.is_truthy() && rhs.is_truthy())),
        BinOp::Or  => Ok(Value::Bool(lhs.is_truthy() || rhs.is_truthy())),
        BinOp::RangeEx => match (&lhs, &rhs) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::make_list((*a..*b).map(Value::Int).collect())),
            _ => Err(GravError::Runtime("range requires integers".into())),
        },
        BinOp::RangeIn => match (&lhs, &rhs) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::make_list((*a..=*b).map(Value::Int).collect())),
            _ => Err(GravError::Runtime("range requires integers".into())),
        },
        BinOp::NullCoalesce => {
            if matches!(lhs, Value::Null) { Ok(rhs) } else { Ok(lhs) }
        },
        // Bitwise operators — integer only
        BinOp::BitAnd => match (&lhs, &rhs) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a & b)),
            _ => Err(GravError::Runtime(format!("bitwise AND requires integers, got {} and {}", lhs.type_name(), rhs.type_name()))),
        },
        BinOp::BitOr => match (&lhs, &rhs) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a | b)),
            _ => Err(GravError::Runtime(format!("bitwise OR requires integers, got {} and {}", lhs.type_name(), rhs.type_name()))),
        },
        BinOp::BitXor => match (&lhs, &rhs) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a ^ b)),
            _ => Err(GravError::Runtime(format!("bitwise XOR requires integers, got {} and {}", lhs.type_name(), rhs.type_name()))),
        },
        BinOp::Shl => match (&lhs, &rhs) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a << b)),
            _ => Err(GravError::Runtime(format!("left shift requires integers, got {} and {}", lhs.type_name(), rhs.type_name()))),
        },
        BinOp::Shr => match (&lhs, &rhs) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a >> b)),
            _ => Err(GravError::Runtime(format!("right shift requires integers, got {} and {}", lhs.type_name(), rhs.type_name()))),
        },
    }
}

fn numeric_op(op: BinOp, lhs: Value, rhs: Value) -> GravResult<Value> {
    match (&lhs, &rhs) {
        (Value::Int(a),   Value::Int(b))   => Ok(Value::Int(match op {
            BinOp::Sub => a - b,
            BinOp::Div => if *b == 0 { return Err(GravError::Runtime("division by zero".into())); } else { a / b },
            BinOp::Rem => if *b == 0 { return Err(GravError::Runtime("modulo by zero".into())); } else { a % b },
            _ => unreachable!(),
        })),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(match op {
            BinOp::Sub => a - b,
            BinOp::Div => a / b,
            BinOp::Rem => a % b,
            _ => unreachable!(),
        })),
        (Value::Int(a), Value::Float(b))   => Ok(Value::Float(match op {
            BinOp::Sub => *a as f64 - b,
            BinOp::Div => *a as f64 / b,
            BinOp::Rem => *a as f64 % b,
            _ => unreachable!(),
        })),
        (Value::Float(a), Value::Int(b))   => Ok(Value::Float(match op {
            BinOp::Sub => a - *b as f64,
            BinOp::Div => a / *b as f64,
            BinOp::Rem => a % *b as f64,
            _ => unreachable!(),
        })),
        _ => Err(GravError::Runtime(format!("cannot apply {:?} to {} and {}", op, lhs.type_name(), rhs.type_name()))),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::BinOp;

    // ── type_name ────────────────────────────────────────────────────────────

    #[test]
    fn value_type_names() {
        assert_eq!(Value::Null.type_name(), "null");
        assert_eq!(Value::Bool(true).type_name(), "bool");
        assert_eq!(Value::Int(42).type_name(), "int");
        assert_eq!(Value::Float(3.14).type_name(), "float");
        assert_eq!(Value::make_str("hi").type_name(), "str");
        assert_eq!(Value::make_list(vec![]).type_name(), "list");
        assert_eq!(Value::make_map(HashMap::new()).type_name(), "map");
    }

    // ── is_truthy ────────────────────────────────────────────────────────────

    #[test]
    fn value_truthiness() {
        assert!(!Value::Null.is_truthy());
        assert!(!Value::Bool(false).is_truthy());
        assert!(Value::Bool(true).is_truthy());
        assert!(!Value::Int(0).is_truthy());
        assert!(Value::Int(1).is_truthy());
        assert!(Value::Int(-1).is_truthy());
        assert!(!Value::Float(0.0).is_truthy());
        assert!(Value::Float(1.0).is_truthy());
        assert!(!Value::make_str("").is_truthy());
        assert!(Value::make_str("hello").is_truthy());
    }

    #[test]
    fn value_truthy_list() {
        assert!(!Value::make_list(vec![]).is_truthy());
        assert!(Value::make_list(vec![Value::Int(1)]).is_truthy());
    }

    #[test]
    fn value_truthy_map() {
        assert!(!Value::make_map(HashMap::new()).is_truthy());
        let mut m = HashMap::new();
        m.insert("k".into(), Value::Int(1));
        assert!(Value::make_map(m).is_truthy());
    }

    // ── conversions ──────────────────────────────────────────────────────────

    #[test]
    fn value_as_int() {
        assert_eq!(Value::Int(42).as_int(), Some(42));
        assert_eq!(Value::Float(3.9).as_int(), Some(3));
        assert_eq!(Value::Null.as_int(), None);
        assert_eq!(Value::Bool(true).as_int(), None);
    }

    #[test]
    fn value_as_float() {
        assert_eq!(Value::Float(3.14).as_float(), Some(3.14));
        assert_eq!(Value::Int(42).as_float(), Some(42.0));
        assert_eq!(Value::Null.as_float(), None);
    }

    #[test]
    fn value_as_str() {
        assert_eq!(Value::make_str("hello").as_str(), Some("hello"));
        assert_eq!(Value::Null.as_str(), None);
        assert_eq!(Value::Int(42).as_str(), None);
    }

    // ── Display ──────────────────────────────────────────────────────────────

    #[test]
    fn value_display() {
        assert_eq!(Value::Int(42).to_string(), "42");
        assert_eq!(Value::Bool(true).to_string(), "true");
        assert_eq!(Value::Null.to_string(), "null");
        assert_eq!(Value::Float(3.14).to_string(), "3.14");
        assert_eq!(Value::make_str("hi").to_string(), "hi");
    }

    #[test]
    fn value_display_list() {
        let v = Value::make_list(vec![Value::Int(1), Value::Int(2)]);
        assert_eq!(v.to_string(), "[1, 2]");
    }

    // ── constructors ─────────────────────────────────────────────────────────

    #[test]
    fn make_str_works() {
        let v = Value::make_str("test");
        assert_eq!(v.as_str(), Some("test"));
    }

    #[test]
    fn make_list_works() {
        let v = Value::make_list(vec![Value::Int(1), Value::Int(2)]);
        if let Value::List(rc) = v {
            assert_eq!(rc.borrow().len(), 2);
        } else {
            panic!("expected List");
        }
    }

    #[test]
    fn make_map_works() {
        let mut m = HashMap::new();
        m.insert("key".to_string(), Value::Int(42));
        let v = Value::make_map(m);
        if let Value::Map(rc) = v {
            assert_eq!(rc.borrow().get("key"), Some(&Value::Int(42)));
        } else {
            panic!("expected Map");
        }
    }

    // ── PartialEq ────────────────────────────────────────────────────────────

    #[test]
    fn value_eq_primitives() {
        assert_eq!(Value::Null, Value::Null);
        assert_eq!(Value::Bool(true), Value::Bool(true));
        assert_ne!(Value::Bool(true), Value::Bool(false));
        assert_eq!(Value::Int(5), Value::Int(5));
        assert_ne!(Value::Int(5), Value::Int(6));
        assert_eq!(Value::Float(1.5), Value::Float(1.5));
        assert_eq!(Value::make_str("a"), Value::make_str("a"));
        assert_ne!(Value::make_str("a"), Value::make_str("b"));
    }

    #[test]
    fn value_eq_int_float_cross() {
        assert_eq!(Value::Int(3), Value::Float(3.0));
        assert_eq!(Value::Float(5.0), Value::Int(5));
    }

    #[test]
    fn value_ne_different_types() {
        assert_ne!(Value::Int(1), Value::Bool(true));
        assert_ne!(Value::Null, Value::Int(0));
    }

    // ── PartialOrd ───────────────────────────────────────────────────────────

    #[test]
    fn value_ord() {
        assert!(Value::Int(3) < Value::Int(5));
        assert!(Value::Float(1.5) < Value::Float(2.5));
        assert!(Value::Int(1) < Value::Float(2.5));
        assert!(Value::make_str("a") < Value::make_str("b"));
    }

    // ── apply_binop: arithmetic ──────────────────────────────────────────────

    #[test]
    fn binop_add_int() {
        assert_eq!(apply_binop(BinOp::Add, Value::Int(2), Value::Int(3)).unwrap(), Value::Int(5));
    }

    #[test]
    fn binop_add_float() {
        assert_eq!(
            apply_binop(BinOp::Add, Value::Float(1.5), Value::Float(2.5)).unwrap(),
            Value::Float(4.0)
        );
    }

    #[test]
    fn binop_add_str() {
        let r = apply_binop(BinOp::Add, Value::make_str("hello"), Value::make_str(" world")).unwrap();
        assert_eq!(r.as_str(), Some("hello world"));
    }

    #[test]
    fn binop_add_str_and_int() {
        let r = apply_binop(BinOp::Add, Value::make_str("n="), Value::Int(42)).unwrap();
        assert_eq!(r.as_str(), Some("n=42"));
    }

    #[test]
    fn binop_sub() {
        assert_eq!(apply_binop(BinOp::Sub, Value::Int(10), Value::Int(3)).unwrap(), Value::Int(7));
    }

    #[test]
    fn binop_mul() {
        assert_eq!(apply_binop(BinOp::Mul, Value::Int(4), Value::Int(5)).unwrap(), Value::Int(20));
    }

    #[test]
    fn binop_mul_str_repeat() {
        let r = apply_binop(BinOp::Mul, Value::make_str("ab"), Value::Int(3)).unwrap();
        assert_eq!(r.as_str(), Some("ababab"));
    }

    #[test]
    fn binop_div() {
        assert_eq!(apply_binop(BinOp::Div, Value::Int(10), Value::Int(3)).unwrap(), Value::Int(3));
    }

    #[test]
    fn binop_div_by_zero() {
        assert!(apply_binop(BinOp::Div, Value::Int(10), Value::Int(0)).is_err());
    }

    #[test]
    fn binop_rem() {
        assert_eq!(apply_binop(BinOp::Rem, Value::Int(10), Value::Int(3)).unwrap(), Value::Int(1));
    }

    #[test]
    fn binop_rem_by_zero() {
        assert!(apply_binop(BinOp::Rem, Value::Int(10), Value::Int(0)).is_err());
    }

    #[test]
    fn binop_pow() {
        assert_eq!(apply_binop(BinOp::Pow, Value::Int(2), Value::Int(10)).unwrap(), Value::Int(1024));
    }

    // ── apply_binop: comparison ──────────────────────────────────────────────

    #[test]
    fn binop_eq() {
        assert_eq!(apply_binop(BinOp::Eq, Value::Int(5), Value::Int(5)).unwrap(), Value::Bool(true));
        assert_eq!(apply_binop(BinOp::Eq, Value::Int(5), Value::Int(3)).unwrap(), Value::Bool(false));
    }

    #[test]
    fn binop_ne() {
        assert_eq!(apply_binop(BinOp::Ne, Value::Int(5), Value::Int(3)).unwrap(), Value::Bool(true));
    }

    #[test]
    fn binop_lt_gt() {
        assert_eq!(apply_binop(BinOp::Lt, Value::Int(3), Value::Int(5)).unwrap(), Value::Bool(true));
        assert_eq!(apply_binop(BinOp::Gt, Value::Int(3), Value::Int(5)).unwrap(), Value::Bool(false));
        assert_eq!(apply_binop(BinOp::Le, Value::Int(5), Value::Int(5)).unwrap(), Value::Bool(true));
        assert_eq!(apply_binop(BinOp::Ge, Value::Int(5), Value::Int(5)).unwrap(), Value::Bool(true));
    }

    // ── apply_binop: logical ─────────────────────────────────────────────────

    #[test]
    fn binop_and_or() {
        assert_eq!(apply_binop(BinOp::And, Value::Bool(true), Value::Bool(false)).unwrap(), Value::Bool(false));
        assert_eq!(apply_binop(BinOp::Or, Value::Bool(true), Value::Bool(false)).unwrap(), Value::Bool(true));
        assert_eq!(apply_binop(BinOp::And, Value::Bool(true), Value::Bool(true)).unwrap(), Value::Bool(true));
        assert_eq!(apply_binop(BinOp::Or, Value::Bool(false), Value::Bool(false)).unwrap(), Value::Bool(false));
    }

    // ── apply_binop: null coalesce ───────────────────────────────────────────

    #[test]
    fn binop_null_coalesce() {
        assert_eq!(apply_binop(BinOp::NullCoalesce, Value::Null, Value::Int(42)).unwrap(), Value::Int(42));
        assert_eq!(apply_binop(BinOp::NullCoalesce, Value::Int(1), Value::Int(42)).unwrap(), Value::Int(1));
    }

    // ── apply_binop: mixed int/float ─────────────────────────────────────────

    #[test]
    fn binop_mixed_int_float() {
        assert_eq!(apply_binop(BinOp::Add, Value::Int(1), Value::Float(2.5)).unwrap(), Value::Float(3.5));
        assert_eq!(apply_binop(BinOp::Sub, Value::Float(5.0), Value::Int(2)).unwrap(), Value::Float(3.0));
        assert_eq!(apply_binop(BinOp::Mul, Value::Int(3), Value::Float(2.0)).unwrap(), Value::Float(6.0));
    }

    // ── apply_binop: range ───────────────────────────────────────────────────

    #[test]
    fn binop_range_exclusive() {
        let r = apply_binop(BinOp::RangeEx, Value::Int(1), Value::Int(4)).unwrap();
        if let Value::List(rc) = r {
            let list = rc.borrow();
            assert_eq!(list.len(), 3);
            assert_eq!(list[0], Value::Int(1));
            assert_eq!(list[2], Value::Int(3));
        } else {
            panic!("expected List");
        }
    }

    #[test]
    fn binop_range_inclusive() {
        let r = apply_binop(BinOp::RangeIn, Value::Int(1), Value::Int(3)).unwrap();
        if let Value::List(rc) = r {
            assert_eq!(rc.borrow().len(), 3);
        } else {
            panic!("expected List");
        }
    }

    // ── apply_binop: type errors ─────────────────────────────────────────────

    #[test]
    fn binop_type_error() {
        assert!(apply_binop(BinOp::Sub, Value::make_str("a"), Value::Int(1)).is_err());
        assert!(apply_binop(BinOp::Div, Value::Bool(true), Value::Int(1)).is_err());
    }
}
