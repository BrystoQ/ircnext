//! Codes des réponses numériques utilisées par le client.
//!
//! Référence : <https://modern.ircdocs.horse/#numerics>

pub const RPL_WELCOME: u16 = 1;
pub const RPL_ISUPPORT: u16 = 5;
pub const RPL_UMODEIS: u16 = 221;
pub const RPL_AWAY: u16 = 301;
pub const RPL_UNAWAY: u16 = 305;
pub const RPL_NOWAWAY: u16 = 306;
pub const RPL_ENDOFWHO: u16 = 315;
pub const RPL_CHANNELMODEIS: u16 = 324;
pub const RPL_CREATIONTIME: u16 = 329;
pub const RPL_NOTOPIC: u16 = 331;
pub const RPL_TOPIC: u16 = 332;
pub const RPL_TOPICWHOTIME: u16 = 333;
pub const RPL_NAMREPLY: u16 = 353;
pub const RPL_WHOSPCRPL: u16 = 354;
pub const RPL_ENDOFNAMES: u16 = 366;
pub const RPL_MOTD: u16 = 372;
pub const RPL_MOTDSTART: u16 = 375;
pub const RPL_ENDOFMOTD: u16 = 376;
pub const ERR_NOMOTD: u16 = 422;
pub const ERR_ERRONEUSNICKNAME: u16 = 432;
pub const ERR_NICKNAMEINUSE: u16 = 433;
pub const ERR_NICKCOLLISION: u16 = 436;
pub const ERR_UNAVAILRESOURCE: u16 = 437;
pub const RPL_MONONLINE: u16 = 730;
pub const RPL_MONOFFLINE: u16 = 731;
pub const RPL_MONLIST: u16 = 732;
pub const RPL_ENDOFMONLIST: u16 = 733;
pub const ERR_MONLISTFULL: u16 = 734;
pub const RPL_LOGGEDIN: u16 = 900;
pub const RPL_LOGGEDOUT: u16 = 901;
pub const RPL_SASLSUCCESS: u16 = 903;
pub const ERR_SASLFAIL: u16 = 904;
pub const ERR_SASLTOOLONG: u16 = 905;
pub const ERR_SASLABORTED: u16 = 906;
pub const ERR_SASLALREADY: u16 = 907;
pub const RPL_SASLMECHS: u16 = 908;

/// Indique si un code numérique est une erreur (plages 400–599 et 902–908 hors succès).
pub fn is_error(code: u16) -> bool {
    (400..600).contains(&code) || matches!(code, 902 | 904..=907)
}
