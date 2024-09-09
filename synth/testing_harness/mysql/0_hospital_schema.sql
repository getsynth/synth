drop table if exists hospitals;

create table hospitals
(
    id            int primary key,
    hospital_name varchar(255),
    address       varchar(255)
);

drop table if exists doctors;

create table doctors
(
    id          int primary key,
    hospital_id int,
    name        varchar(255),
    date_joined date
);

drop table if exists patients;

create table patients
(
    id          int primary key,
    doctor_id   int,
    name        varchar(255),
    date_joined date,
    address     varchar(255),
    phone       varchar(20),
    ssn         varchar(12),
    freckles    tinyint
);
