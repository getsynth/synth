---
title: Seeding test databases in 2021 - best practices
author_url: https://github.com/brokad/
author: Damien B. (@brokad)
author_image_url: https://avatars.githubusercontent.com/u/13315034?v=4
tags: [postgres, test data, data generation, tutorial, beginners guide, seeding, prisma, schema, data model, orm]
description: In this tutorial, we'll learn how to design a Prisma data model for a basic message board and how to seed test databases with mock data using open-source tools.
image: https://storage.googleapis.com/getsynth-public/media/orm_small.jpg
hide_table_of_contents: false
---

![Seeding test databases in 2021 - best practices](media/orm_small.jpg)

In this tutorial, we'll learn how to design
the [Prisma data model][prisma-schema] for a basic message board and how to seed
databases with the open-source tool [`synth`][synth-repo] and generate mock data to
test our code.

The code for the example we are working with here can be accessed in
the [examples repository on GitHub][repo-complete].

## Data modeling is not boring

### What is a data model?

Data modeling (in the context of databases and this tutorial) refers to the
practice of formalizing a collection of entities, their properties and relations
between one another. It is an almost mathematical process (borrowing a lot of
language from [set theory][set-theory]) but that should not scare you. When it
comes down to it, it is exceedingly simple and quickly becomes more of an art
than a science.

The crux of the problem of data modeling is to summarize and write down what
constitutes useful entities and how they relate to one another in a graph of
connections.

You may wonder what constitutes a *useful* entity. It is indeed the toughest
question to answer. It is very difficult to tackle it without a good combined
idea of what you are building, the database you are building on top of and what
the most common queries, operations and aggregate statistics are. There
are [many resources][data-modeling-101] out there that will guide you through
answering that question. Here we'll start with the beginning: why is it needed?

### Why do I need a data model?

Often times, getting the data model of your application right is crucial to its
performance. A bad data model for your backend can mean it gets crippled by
seemingly innocuous tasks. On the other hand, a good grasp on data modeling will
make your life as a developer 1000 times easier. A good data model is not a
source of constant pain, letting you develop and expand without slowing you
down. It just is one of those things that pays out compounding returns.

Plus, there are nowadays many open-source tools that make building applications
on top of data models really enjoyable. One of them is Prisma.

### Prisma is awesome

[Prisma][prisma] is an ORM, an *object relational mapping*. It is a powerful
framework that lets you specify your data model using a database agnostic domain
specific language (called the [Prisma schema][prisma-schema]). It
uses [pluggable generators][prisma-generate] to build a nice javascript API and
[typescript][typescript] bindings for your data model. Hook that up to your IDE
and you get amazing code completion that is tailored to your data model, in
addition to a powerful query engine.

Let's walk through a example. We want to get a sense for what it'll take to
design the data model for a simple message board a little like [Reddit][reddit]
or [YCombinator's Hacker News][hacker-news]. At the very minimum, we want to
have a concept of *users*: people should be able to register for an account.
Beyond that, we need a concept of *posts*: some structure, attached to users,
that holds the content they publish.

Using the [Prisma schema][prisma-schema] language, which is very expressive even
if you haven't seen it before, our first go at writing down a `User` entity
might look something like this:

```graphql
model User {
  objectId  Bytes    @id @map("_id")
  id        Int      @unique @default(autoincrement())
  createdAt DateTime @default(now())
  email     String   @unique
  nickname  String
  posts     Post[]
}
```

In other words, our `User` entity has properties `id` (a database internal
unique identifier), `createdAt` (a timestamp, defaulting to now if not
specified, that marks the creation time of the user's account), `email` (the
user-specified email address, given on registration) which is required to be
unique (no two users can share an email address) and `nickname` (the user
specified display name, given on registration).

In addition, it has a property `posts` which links a user with its posts through
the `Post` entity. We may come up with something like this for the `Post`
entity:

```graphql
model Post {
  objectId  Bytes    @id @map("_id")
  id        Int      @unique @default(autoincrement())
  postedAt  DateTime @default(now())
  title     String
  author    User     @relation(fields: [authorId], references: [id])
  authorId  Int
}
```

In other words, our `Post` entity has properties `id` (a database internal
unique identifier); `postedAt` (a timestamp, defaulting to now if not specified,
that marks the time at which the user created the post and published it)
; `title` (the title of the post); `author` and `authorId` which specify a
one-to-many relationship between users and posts.

:::note

You may have noticed that the `User` and `Post` models have an attribute which
we haven't mentioned. The `objectId` property is
an [internal unique identifier][mongodb-objectid] used by [mongoDB][mongodb] 
(the database we're choosing to implement our data model on in this tutorial).

:::

Let's look closer at these last two properties `author` and `authorId`. There is
a significant difference between them with respect to how they are implemented
in the database. Remember that, at the end of the day, our data model will need
to be realized into a database. Because we're using [Prisma][prisma], a lot of
these details are abstracted away from us. In this case,
the [prisma code-generator][prisma-generate] will handle `author` and `authorId`
slightly differently.

The `@relation(...)` attribute on the `author` property is [Prisma][prisma]'s
way of declaring that `authorId` is a [foreign key][foreign-key] field. Because
the type of the `author` property is a `User` entity, [Prisma][prisma]
understands that posts are linked to users via
the [foreign key][foreign-key] `authorId` which maps to the user's `id`, the
associated [primary key][primary-key]. This is an example of
a [one-to-many relation][prisma-one-to-many].

How that relation is implemented is left to [Prisma][prisma] and depends on the
database you choose to use. Since we are using [mongodb][mongodb] here, this is
implemented by [direct object id references][prisma-relation-mongo].

Because our data model encodes the relation between posts and users, looking up
a user's posts is inexpensive. This is the benefit of designing a good data
model for an application: operations you have designed and planned for at this
stage, are optimized for.

To get us started using this [Prisma][prisma] data model in an actual
application, let's create a new `npm` project in an empty directory:

```bash
$ npm init
```

When prompted to specify the entry point, use `src/index.js`. Install some nice
typescript bindings for node with:

```bash
$ npm install --save-dev @types/node typescript
```

Then you can initialize the typescript compiler with

```bash
$ npx tsc --init
```

This creates a `tsconfig.json` file which configures the behavior of the
typescript compiler. Create a directory `src/` and add the following `index.ts`:

```javascript
import {PrismaClient} from '@prisma/client'

const prisma = new PrismaClient()

const main = async () => {
    const user = await prisma.user.findFirst()
    if (user === null) {
        throw Error("No user data.")
    }
    console.log(`found username: ${user.nickname}`)
    process.exit(0)
}

main().catch((e) => {
    console.error(e)
    process.exit(1)
})
```

Then create a `prisma/` directory and add a `schema.prisma` file containing
the Prisma code for the two entities `User` and `Post`. 

Finally, to our `schema.prisma` file, we need to add configuration for our local
dev database and the generation of the client:

```graphql
datasource db {
  provider = "mongodb"
  url      = "mongodb://localhost:27017/board"
}

generator client {
  provider = "prisma-client-js"
  previewFeatures = ["mongodb"]
}
```

[Head over to the repository][repo-schema]
to see an example of the complete file, including the extra configuration.

To build the [Prisma client][prisma-generate], run

```bash
$ npx prisma generate
```

Finally, to run it all, edit your `package.json` file (at the root of your
project's directory). Look for the `"script"` field and modify the `"test"`
script with:

```json
{
  ...
  "test": "tsc --project ./ && node ."
  ... 
}
```

Now all we need is for an instance of [mongoDB][mongodb] to be running while
we're working. We can run that straight from the official docker image:

```bash
$ docker run -d --name message-board-example -p 27017:27017 --rm mongo
```

To run the example do

```bash
$ npm run test

> message-board-example@1.0.0 test /tmp/message-board-example
> tsc --project ./ && node .

Error: No user data.
```

You should see something close to the output of the snippet: our simple code
failed because it is looking for a user that does not exist (yet) in our dev
database. We will [fix that in a little bit](#generate-data-for-your-data-model)
. But first, here's a secret.

## The secret to writing good code

Actually it's no secret at all. It is one of those things that everybody with
software engineering experience knows. The key to writing good code is learning
from your mistakes!

When coding becomes tedious is when it is hard to learn from errors. Usually
this is caused by a lengthy process to go from writing the code to testing it.
This can happen for many reasons: having to wait for the deployment of a backend
in `docker compose`, sitting idly by while your code compiles just to fail at
the end because of a typo, the strong integration of a system with components
external to it, and many more.

The process that goes from the early stages of designing something to verifying
its functionalities and rolling it out, that is what is commonly called
the [development cycle][development-cycle].

It should indeed be a cycle. Once the code is out there, deployed and running,
it gets reviewed for quality and purpose. More often than not this happens
because users break it and give feedback. The outcome of that gets folded in
planning and designing for the next iteration or release.
The [agile philosophy][agile-framework] is built on the idea that this cycle
should be as short as possible.

So that brings the question: how do you make the development cycle as quick as
possible? The faster the cycle is, the better your productivity becomes.

### Testing, testing and more testing

One of the keys to shortening a development cycle is making testing easy. When
playing with databases and data models, it is something that is often hacky. In
fact there are very few tools that let you iterate quickly on data models, much
less *developer-friendly* tools.

The core issue at hand is that between iterations on ideas and features, we will
need to make small and quick changes to our data model. What happens to our
databases and the data in them in that case? Migration is sometimes an option
but is notoriously hard and may not work at all if our changes are significant.

For development purposes the quickest solution is seeding our new data model
with mock data. That way we can test our changes quickly and idiomatically.

## Generate data for your data model

At [Synth][getsynth] we are building a declarative test data generator. It lets
you write your data model in plain **zero-code** [JSON][json] and seed many
relational and non-relational databases with mock data. It is
completely [free and open-source][synth-repo].

Let's take our [data model](#prisma-is-awesome) and seed a
development [mongoDB][docker-mongo] database instance with [Synth][getsynth].
Then we can make our development cycle very short by using
an [npm script][npm-script]
that sets it all up for us whenever we need it.

### Installing `synth`

We'll need the [`synth`][synth-cli] command-line tool to get started. From a
terminal, run:

```bash
$ curl -sSL https://getsynth.com/install | sh
```

This will run you through an install script for the [binary release][binary]
of [`synth`][synth-cli]. If you prefer installing from source, we got you: head
on over to the [Installation][installation]
pages of the official documentation.

Once the installer script is done, try running

```bash
$ synth version
synth 0.5.4
```

to make sure everything works. If it doesn't work, add `$HOME/.local/bin`
to your `$PATH` environment variable with

```bash
$ export PATH=$HOME/.local/bin:$PATH
```

and try again.

### Synth schema

Just like [Prisma][prisma] and its schema DSL, [`synth`][synth-cli] lets you
write down your data model with zero code.

There is one main difference: the [`synth`][synth-cli] schema is aimed at the
generation of data. This means it lets you specify the semantics of your data
model in addition to its entities and relations. The [`synth`][synth-cli] schema
has an understanding of what an email, a username, an address are; whereas the
Prisma schema only cares about top-level types (strings, integers, etc).

Let's navigate to the top of our [example project's][repo-complete] directory
and create a new directory called `synth/` for storing our [`synth`][synth-cli]
schema files.

```
├── package.json
├── package-lock.json
├── tsconfig.json
├── prisma/
├── synth/
└── src/
``` 

Each file we will put in the `synth/` directory that ends in `.json` will be
opened by [`synth`][synth-cli], parsed and interpreted as part of our data
model. The structure of these files is simple: each one represents
a [collection][mongo-collection] in our database.

### Collections

A collection is a single JSON schema file, stored in a namespace directory.
Because collections are formed of many elements, their [Synth][getsynth] schema
type is that of [arrays][synth-array].

To get started, let's create a `User.json` file in the `synth/` directory:

```json synth
{
  "type": "array",
  "length": 1,
  "content": {
    "type": "null"
  }
}
```

Then run

```bash
$ synth generate synth/
{"users":[null]}
```

Let's break this down. Our `User.json` collection schema is a JSON object with
three fields. The `"type"` represents the kind of generator we want. As we said
above, collections must generate arrays. The `"length"` and `"content"`
fields are the parameters we need to specify an [array generator][synth-array].
The `"length"` field specifies how many elements the generated array must have.
The `"content"` specifies from what the elements of the array are generated.

For now the value of `"content"` is a generator of the `null` type. Which is why
our array has `null` as a single element. But we will soon change this.

Note that the value of `"length"` can be another generator. Of course, because
the length of an array is non-negative number, it cannot be just any generator.
But it can be any kind that will generate non-negative numbers. For example

```json synth
{
  "type": "array",
  "length": {
    "type": "number",
    "range": {
      "low": 5,
      "high": 10,
      "step": 1
    }
  },
  "content": {
    "type": "null"
  }
}
```

This now makes our `users` collection variable length. Its length will be
decided by the result of generating a new random integer between 5 and 10.

If you now run

```bash
$ synth generate synth/
{"users":[null,null,null,null,null]}
```

you can see the result of that change.

:::info

By default `synth` fixes the seed of its
internal [PRNG][prng]. This means that, by default, running `synth` many times
on the same input schemas will give the same output data. If you want to
randomize the seed - and thus randomize the result, simply add the
flag [`--random`][synth-cli]:

```bash
$ synth generate synth/ --random
{"users":[null,null,null,null,null,null,null]}

$ synth generate synth/ --random
{"users":[null,null,null,null,null,null,null,null,null]}
```

:::

### Schema nodes

Before we can get our `users` collection to match
our [`User` Prisma model](#prisma-is-awesome), we need to understand how to
generate more kinds of data with `synth`.

Everything that goes into a schema file is a [schema node][synth-schema]. Schema
nodes can be identified by the `"type"` field which specifies which kind of node
it is. The documentation pages have
a [complete taxonomy of schema nodes][synth-generators] and their `"type"`.

### Generating ids

Let's look back at our [`User` model](#prisma-is-awesome). It has four
properties:

- `id`
- `createdAt`
- `email`
- `nickname`

Let's start with `id`. How can we generate that?

The type of the `id` property in the [`User` model](#prisma-is-awesome)
is `Int`:

```graphql
  id        Int      @unique @default(autoincrement())
``` 

and the attribute indicates that the field is meant to increment sequentially,
going through values 0, 1, 2 etc.

The `synth` schema type for numbers is [`number`][synth-number].
Within [`number`][synth-number] there are three varieties of generators:

- [`range`][synth-range]
- [`constant`][synth-constant]
- [`id`][synth-id]

What decides the variant is the presence of the `"range"`, `"constant"`or `"id"`
field in the node's specification.

For example, a [`range`][synth-range] variant would look like

```json synth
{
	"type": "number",
	"range": {
	    "low": 5,
	    "high": 10,
	    "step": 1
	}
}
```

whereas a [`constant`][synth-constant] variant would look like

```json synth
{
    "type": "number",
    "constant": 42
}
```

For the `id` field we should use the [`id`][synth-id] variant, which is
auto-incrementing. Here is an example of [`id`][synth-id] used in an array so we
can see it behaves as expected:

```json synth
{
    "type": "array",
    "length": 10,
    "content": {
        "type": "number",
        "id": {}
    }
}
```

### Generating emails

Let us now look at the [`email`](#prisma-is-awesome) field of
our [`User` model](#prisma-is-awesome):

```graphql
  email     String   @unique
```

Its type in the data model is that of a `String`. The `synth` schema type for
that is [`string`][synth-string].

There are many different variants of [`string`][synth-string] and they are
all [exhaustively documented][synth-string]. The different variants are
identified by the presence of a distinguishing field which can be

- `"faker"`
- `"pattern"`
- [and a lot more][synth-string]...

Since we are interested in generating email addresses, we will be using
the [`"faker"`][synth-faker] variant which leverages a preset collection of
generators for common properties like usernames, addresses and emails:

```json synth
{
    "type": "string",
    "faker": {
        "generator": "safe_email"
    }
}
```

### Generating objects

OK, so we now know how to generate the `id` and the `email` properties of
our [`User` model](#prisma-is-awesome). But we do not yet know how to put them
together in one object. For that we need the [`object`][synth-object] type:

```json synth[User.json]
{
    "type": "object",
    "id": {
        "type": "number",
        "id": {}
    },
    "email": {
        "type": "string",
        "faker": {
            "generator": "safe_email"
        }
    }
}
```

### Leverage the docs

Now we have everything we need to finish writing down
our [`User` model](#prisma-is-awesome) as a `synth` schema. A quick lookup of
the [documentation pages][synth-string] will tell us how to generate
the `createdAt` and `nickname` fields.

Here is the finished result for our `User.json` collection:

```json synth[expect = "unknown variant `date_time`"]
{
    "type": "array",
    "length": 3,
    "content": {
        "type": "object",
        "id": {
            "type": "number",
            "id": {}
        },
        "createdAt": {
            "type": "string",
            "date_time": {
                "format": "%Y-%m-%d %H:%M:%S",
                "begin": "2020-01-01 12:00:00"
            }
        },
        "email": {
            "type": "string",
            "faker": {
                "generator": "safe_email"
            }
        },
        "nickname": {
            "type": "string",
            "faker": {
                "generator": "username"
            }
        }
    }
}
```

:::caution

[`date_time`][synth-datetime] is now a generator on its own and is no longer a subtype of the `string` generator

:::

### Making sure our constraints are satisfied

Looking back at the [`User` model](#prisma-is-awesome) we started from, there's
one thing that we did not quite address yet. The `email` field in the Prisma
schema has the `@unique` attribute:

```graphql
  email     String   @unique
```

This means that, in our data model, no two users can share the same email
address. Yet, we haven't added that constraint anywhere in
our [final `synth` schema](#leverage-the-docs)
for the `User.json` collection.

What we need to use here is [`modifiers`][synth-modifiers]. A modifier is an
attribute that we can add to any `synth` schema type to modify the way it
behaves. There are two modifiers currently supported:

- [`optional`][synth-optional]
- [`unique`][synth-unique]

The [`optional`][synth-optional] modifier is an easy way to make a schema node
randomly generate something or nothing:

```json synth
{
    "type": "number",
    "optional": true,
    "constant": 42
}
```

Whereas the [`unique`][synth-unique] modifier is an easy way to enforce the
constraint that the values generated have no duplication. So all we need to do,
to represent our data model correctly, is to add the [`unique`][synth-unique]
modifier to the `email` field:

```json synth
{
    "type": "string",
    "unique": true,
    "faker": {
        "generator": "safe_email"
    }
}
```

The completed end result for the `User.json`
collection [can be viewed on GitHub here][repo-users-json].

### How to deal with relations

Now that we have set up our `User.json` collection, let's turn our attention to
the [`Post` model](#prisma-is-awesome) and write out the `synth` schema for
the `Post.json` collection.

Here is the end result:

```json synth[expect = "unknown variant `date_time`"]
{
  "type": "array",
  "length": 5,
  "content": {
    "type": "object",
    "id": {
      "type": "number",
      "id": {}
    },
    "postedAt": {
      "type": "string",
      "date_time": {
        "format": "%Y-%m-%d %H:%M:%S",
        "begin": "2020-01-01 12:00:00"
      }
    },
    "title": {
      "type": "string",
      "faker": {
        "generator": "bs"
      }
    },
    "authorId": "@User.content.id"
  }
}
```

:::caution

[`date_time`][synth-datetime] is now a generator on its own and is no longer a subtype of the `string` generator

:::

It all looks pretty similar to the `User.json` collection, except for one
important difference at the line

```json synth
    "authorId": "@User.content.id"
```

The syntax `@...` is `synth`'s way of
specifying [relations between collections][synth-same-as]. Here we are creating
a [many-to-1][n-to-1] relation between the field `authorId`
of the `Post.json` collection and the field `id` of the `User.json`
collection.

The final `Post.json` collection
schema [can be viewed on GitHub here][repo-posts-json].

### Synth generate

Now that our data model is implemented in [Synth][synth-schema], we're ready to
seed our test database with mock data. Here we'll use
the [offical mongo Docker image][docker-mongo], but if you are using a
relational database like [Postgres][docker-postgres]
or [MySQL][docker-mysql], you can follow the same process.

To start the mongo image in the background (if you haven't done so already), run

```bash
$ docker run -d -it -p 27017:27017 --rm mongo
```

Then, to seed the database with `synth` just run

```bash
$ synth generate synth/ --size 1000 --to mongodb://localhost:27017/board
```

That's it! Our test mongo instance is now seeded with the data of around 100
users. [Head over to the examples repository][repo-complete] to see the complete
working example.

## What's next

[Synth][getsynth] is completely free and built in the open by
an [amazing and fast growing community of contributors][synth-contributors].

[Join us in our mission][synth-twitter] to make test data easy and painless! We
also have a very [active Discord server][discord] where many members of the
community would be happy to help if you encounter an issue!

[binary]: https://en.wikipedia.org/wiki/Executable

[set-theory]: https://en.wikipedia.org/wiki/Set_theory

[prisma]: https://www.prisma.io/

[prisma-generate]: https://www.prisma.io/docs/concepts/components/prisma-schema/generators

[prisma-schema]: https://www.prisma.io/docs/concepts/components/prisma-schema

[typescript]: https://www.typescriptlang.org/docs/handbook/typescript-from-scratch.html

[getsynth]: https://getsynth.com

[JSON]: https://www.json.org/json-en.html

[synth-repo]: https://github.com/getsynth/synth

[installation]: /docs/getting_started/installation

[development-cycle]: https://en.wikipedia.org/wiki/Systems_development_life_cycle

[agile-framework]: https://en.wikipedia.org/wiki/Agile_software_development#Iterative,_incremental,_and_evolutionary

[postgres]: https://www.postgresql.org/

[mongodb]: https://www.mongodb.com/

[mongodb-objectid]: https://docs.mongodb.com/manual/reference/method/ObjectId/

[mongo-collection]: https://docs.mongodb.com/manual/core/databases-and-collections/

[reddit]: https://www.reddit.com/

[hacker-news]: https://news.ycombinator.com/

[data-modeling-101]: https://www.prisma.io/dataguide/

[foreign-key]: https://en.wikipedia.org/wiki/Foreign_key

[primary-key]: https://en.wikipedia.org/wiki/Primary_key

[prisma-client]: https://www.prisma.io/docs/concepts/components/prisma-client

[prisma-one-to-many]: https://www.prisma.io/docs/concepts/components/prisma-schema/relations/one-to-one-relations

[table]: https://en.wikipedia.org/wiki/Table_(information)

[synth-array]: /docs/content/array

[prng]: https://en.wikipedia.org/wiki/Pseudorandom_number_generator

[synth-schema]: /docs/getting_started/schema

[synth-generators]: /docs/content/null

[synth-number]: /docs/content/number

[synth-id]: /docs/content/number#id

[synth-constant]: /docs/content/number#constant

[synth-range]: /docs/content/number#range

[synth-string]: /docs/content/string

[synth-faker]: /docs/content/string#faker

[synth-object]: /docs/content/object

[synth-modifiers]: /docs/content/modifiers

[synth-datetime]: /docs/content/date-time

[synth-optional]: /docs/content/modifiers#optional

[synth-unique]: /docs/content/modifiers#unique

[synth-same-as]: /docs/content/same-as

[n-to-1]: https://www.prisma.io/docs/concepts/components/prisma-schema/relations/one-to-many-relations

[discord]: https://discord.com/invite/H33rRDTm3p

[synth-contributors]: https://github.com/getsynth/synth#contributors-

[synth-twitter]: https://twitter.com/getsynth

[synth-cli]: /docs/getting_started/command-line

[docker-mongo]: https://hub.docker.com/_/mongo

[docker-mysql]: https://hub.docker.com/_/mysql

[docker-postgres]: https://hub.docker.com/_/postgres

[repo-users-json]: https://github.com/getsynth/synth/tree/master/examples/message_board/synth/User.json

[repo-posts-json]: https://github.com/getsynth/synth/tree/master/examples/message_board/synth/Post.json

[repo-schema]: https://github.com/getsynth/synth/tree/master/examples/message_board/prisma/schema.prisma

[repo-complete]: https://github.com/getsynth/synth/tree/master/examples/message_board

[npm-script]: https://github.com/brokad/synth/tree/master/examples/message_board/helpers/db.js

[prisma-relation-mongo]: https://www.prisma.io/docs/concepts/components/prisma-schema/relations/one-to-many-relations
