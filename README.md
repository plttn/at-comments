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

For immediate indexing without waiting for the 15-minute polling interval, the blog's client-side
code includes fallback logic to check the RSS feed directly if a post isn't found in the database.

## Configuration

Configure in `Rocket.toml`:

- `poster_handle`: Your Bluesky handle (e.g., "jack.is")
- `target_emoji`: Emoji prefix to identify blog posts (e.g., "📝")
- `blog_domain`: Your blog's domain to match URLs (e.g., "jack.is")

## Local Postgres Runner

If you want to run the app locally without a DB sidecar setup, use:

```bash
mise run db-setup
```

This task:

- Starts a Postgres container on `localhost:54329`
- Creates the `posts` table directly (ignores migrations)
- Seeds sample rows
- Uses `ROCKET_DATABASES` from `mise.toml`

Then start the app:

```bash
mise run dev
```

Quick test:

```bash
curl http://127.0.0.1:4321/slug/hello-world
```

To stop/remove the local Postgres container:

```bash
mise run db-down
```

## Caveats

This work so far makes a whole lot of assumptions. It only tracks a single user
currently. You could probably add more fields and handle multiple Bluesky users
to be tracked, but given that I'm probably the only one that will be using it,
I'm not too worried about that limitation.
