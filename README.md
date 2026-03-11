# Bluesky Comments Indexer

This is a very _very_ specific usecase of mine for my Bluesky comments page.

## Why?

I use Bluesky as the comments section on my blog. I don't want to have to go and
update the info on the post when [Echofeed](https://echofeed.app) sends the post
to my Bluesky account. Recently, Bryan Newbold skeeted that you shouldn't use
the full text search API for automation.

```ts
const getPostAndThreadData = async (
  slug: string,
  setThread,
  setUri,
  setError,
) => {
  const agent = new Agent("https://public.api.bsky.app");
  const assembledUrl = "https://jack.is/posts/" + slug.toString();
  try {
    const response = await agent.app.bsky.feed.searchPosts({
      q: "📝",
      author: "did:plc:cwdkf4xxjpznceembuuspt3d",
      sort: "latest",
      limit: 1,
      url: assembledUrl,
    });
    const uri = response.data.posts[0].uri;
    setUri(uri);
    try {
      const thread = await getPostThread(uri);
      setThread(thread);
    } catch (err) {
      setError("Error loading comments");
    }
  } catch (err) {
    setError(err);
  }
};
```

"oh no" was my reaction, so I decided to put together a little database project
that polls my Bluesky RSS feed every 15 minutes and persists all the relevant post
rkeys in a database, so that my clientside code can just make a request to the API server.

## How It Works

The service polls the Bluesky RSS feed at `https://bsky.app/profile/{handle}/rss` every 15 minutes.
It looks for posts starting with 📝 that contain links to the configured blog domain, extracts the slug
from the URL and the rkey from the post's AT-URI, then stores the mapping in a PostgreSQL database.

The API endpoint `GET /slug/<slug>` returns the post metadata (rkey, time_us) which the client-side
code uses to fetch the full post thread from Bluesky.

For "cache busting", whenever a request is made to a slug lookup, if it's not found
in the database, it will instead poll the feed directly and check again to see
if there's a fresh post, save it to the DB, then return.

## Configuration

Configuration can be done via environment variables with the `ATC_` prefix or a `Settings.toml` file.

The available options are as follows:

| Variable                      | Description                                                                                                         |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| DATABASE_URL / database.url   | The Postgres connection string                                                                                      |
| APP_ADDRESS / app.address     | Address to bind to                                                                                                  |
| APP_PORT / app.port           | Port to bind to                                                                                                     |
| POLLER_HANDLE / poller.handle | Bluesky username to check the RSS feed of                                                                           |
| POLLER_EMOJI / poller.emoji   | The emoji to search for                                                                                             |
| POLLER_DOMAIN / poller.domain | Scoping to ensure that only the correct post is found. If the post does not link to this domain, it will be skipped |

## Caveats

This work so far makes a whole lot of assumptions. It only tracks a single user
currently. You could probably add more fields and handle multiple Bluesky users
to be tracked, but given that I'm probably the only one that will be using it,
I'm not too worried about that limitation.
