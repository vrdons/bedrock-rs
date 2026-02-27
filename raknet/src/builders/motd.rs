use bytes::Bytes;

/// Builder for RakNet/Bedrock advertisement payload (MOTD).
///
/// The resulting string format is:
/// `MCPE;<motd>;<protocol_version>;<mc_version>;<online_players>;<max_players>;<server_guid>;<sub_motd>;<game_mode>;<nintendo_limited>;19132;19133`
#[derive(Debug, Clone)]
pub struct RaknetMotd {
    motd: String,
    protocol_version: u32,
    mc_version: String,
    online_players: u32,
    max_players: u32,
    server_guid: u64,
    sub_motd: String,
    game_mode: String,
    nintendo_limited: u8,
    ipv4_port: u16,
    ipv6_port: u16,
}

impl RaknetMotd {
    /// Creates a new MOTD builder with sensible defaults.
    pub fn new(motd: impl Into<String>, sub_motd: impl Into<String>) -> Self {
        Self {
            motd: motd.into(),
            protocol_version: 527,
            mc_version: "1.19.1".to_string(),
            online_players: 0,
            max_players: 10,
            server_guid: 13253860892328930865,
            sub_motd: sub_motd.into(),
            game_mode: "Survival".to_string(),
            nintendo_limited: 1,
            ipv4_port: 19132,
            ipv6_port: 19133,
        }
    }

    pub fn motd(mut self, motd: impl Into<String>) -> Self {
        self.motd = motd.into();
        self
    }

    pub fn protocol_version(mut self, protocol_version: u32) -> Self {
        self.protocol_version = protocol_version;
        self
    }

    pub fn mc_version(mut self, mc_version: impl Into<String>) -> Self {
        self.mc_version = mc_version.into();
        self
    }

    pub fn online_players(mut self, online_players: u32) -> Self {
        self.online_players = online_players;
        self
    }

    pub fn max_players(mut self, max_players: u32) -> Self {
        self.max_players = max_players;
        self
    }

    pub fn server_guid(mut self, server_guid: u64) -> Self {
        self.server_guid = server_guid;
        self
    }

    pub fn sub_motd(mut self, sub_motd: impl Into<String>) -> Self {
        self.sub_motd = sub_motd.into();
        self
    }

    pub fn game_mode(mut self, game_mode: impl Into<String>) -> Self {
        self.game_mode = game_mode.into();
        self
    }

    pub fn nintendo_limited(mut self, nintendo_limited: u8) -> Self {
        self.nintendo_limited = nintendo_limited;
        self
    }

    pub fn ipv4_port(mut self, ipv4_port: u16) -> Self {
        self.ipv4_port = ipv4_port;
        self
    }

    pub fn ipv6_port(mut self, ipv6_port: u16) -> Self {
        self.ipv6_port = ipv6_port;
        self
    }

    /// Builds the MOTD payload string.
    pub fn build(self) -> String {
        format!(
            "MCPE;{};{};{};{};{};{};{};{};{};{};{}",
            self.motd,
            self.protocol_version,
            self.mc_version,
            self.online_players,
            self.max_players,
            self.server_guid,
            self.sub_motd,
            self.game_mode,
            self.nintendo_limited,
            self.ipv4_port,
            self.ipv6_port
        )
    }

    /// Builds the MOTD payload as bytes for `RaknetListenerConfigBuilder::advertisement`.
    pub fn marshal(self) -> Vec<u8> {
        self.build().into_bytes()
    }

    /// Builds the MOTD payload as `bytes::Bytes`.
    pub fn bytes(self) -> Bytes {
        Bytes::from(self.marshal())
    }
}
