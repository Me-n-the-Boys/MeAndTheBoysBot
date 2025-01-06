# Me and the Boys Bot
This is a bot that I made for a friend's discord server.
Right now it's a relatively simple bot that can only do a few things:
- Only works on a single Guild.
- Have users create new voice channels, by joining ONE creator channel
  - Users will have permission to change the permissions and settings of the channel they create (for this reason ONLY the bot required Administrator permissions)
  - Users will be moved to their newly created voice channel
  - Temporary Voice channels are kept track of and deleted after 15 seconds of the last person leaving and no-one rejoining.
- A leveling system that gives users XP for being in voice channels and chatting
  - Voice Xp
    - One second of being in a voice channel gives 1 Xp
    - Voice Xp is given on every leave
      - If you directly swap from one voice channel to another, it's not a leave.
      - Probably leaving and joining another channel really quickly also won't count.
    - Certain Channels can be excluded from giving Xp
      - Joining an excluded channel will also apply the currently outstanding Xp
  - Chat Xp (not implemented yet)
    - Every message gives 1 Xp
    - Messages sent faster than 1 minute won't count. 